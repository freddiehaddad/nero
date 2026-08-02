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
//! Translated: 7 small, self-contained predicate functions, none
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
//! - [`typebuf_typed`]/[`typebuf_maplen`] - whether there are (and how
//!   many) untyped (mapped, or from `:normal`) characters in the
//!   typeahead buffer. The new `TYPEBUF: GlobalCell<
//!   crate::input_defs::TypebufT>` instance's own `tb_maplen` starts
//!   at `0` and is only ever changed inside `ins_typebuf` (not yet
//!   translated), so `typebuf_typed()` is always `true` and
//!   `typebuf_maplen()` always `0` today.
//! - [`typebuf_changed`] - whether the typeahead buffer changed since
//!   a given `tb_change_cnt` snapshot (e.g. from a client message or
//!   `feedkeys()`). A real, generically-callable predicate (depends on
//!   its own `tb_change_cnt` parameter, not just untranslated internal
//!   state), needing only `TYPEBUF.tb_change_cnt` plus the
//!   already-existing `crate::globals::Globals::typebuf_was_filled`.
//!
//! Also translated: [`stuff_readbuff`]/[`stuff_redo_readbuff`]/
//! [`stuffchar_readbuff`]/[`stuffnum_readbuff`] (`stuffReadbuff`/
//! `stuffRedoReadbuff`/`stuffcharReadbuff`/`stuffnumReadbuff` -
//! adapted to snake_case, matching this crate's usual convention even
//! though the originals are themselves camelCase - built on the
//! private `add_buff`/`add_num_buff`/`add_byte_buff`/`add_char_buff`
//! helpers) - now give `READBUF1`/`READBUF2` real, observable content.
//! Nothing yet CONSUMES them (`read_readbuf`/`vgetc` aren't
//! translated), so calling these has no downstream effect today, but
//! the buffers themselves now behave faithfully.
//! `add_buff`'s own `bh_index`-based prefix-compaction (the original's
//! `bh_index != 0` branch, needed only when reading has partially
//! consumed the first block) is not modeled: nothing yet reads from
//! either buffer, so `bh_index` can only ever be its own zero default
//! today - the whole branch is unreachable, not a narrowing. The
//! original's own block-capacity-tracking fields (`bh_space`/
//! `bh_create_newblock`/the dummy `bh_first` node) don't exist at all
//! in this crate's own [`crate::input_defs::BuffheaderT`] redesign (a
//! plain `Vec<BuffblockT>`), so `add_buff` is simply `blocks.push(...)`,
//! since those fields were purely a memory-layout optimization in the
//! original (avoiding over-allocating many small blocks), never a
//! semantic requirement observable by any reader.
//!
//! Deferred: everything else - `vgetc`/`vgetorpeek` and the whole
//! typeahead-buffer/mapping-application machinery, `stuffReadbuffLen`/
//! `stuffReadbuffSpec`/`stuffescaped` (need the `add_last_insert`/
//! `last_insert_ga` "last insert text" tracking pair, or
//! `mb_cptr2char_adv`, not yet examined), `ins_typebuf` itself (the
//! real typeahead-buffer WRITE path - needs `state_no_longer_safe`,
//! not yet examined), `redobuff`/`old_redobuff`/`recordbuff` (the "."
//! and macro-recording buffers), `openscript`/`open_scriptin`/
//! `close_all_scripts` (real script-file I/O).

use crate::globals::GlobalCell;
use crate::input_defs::{BuffheaderT, TypebufT};

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

/// The typeahead buffer (`typebuf`). File-static in the original
/// (`static typebuf_T typebuf = { ... }`, zero-initialized here as
/// `TypebufT::default()` matches this crate's own "Default mirrors
/// raw C zero-init" convention).
static TYPEBUF: GlobalCell<TypebufT> = GlobalCell::new(TypebufT {
    tb_buf: Vec::new(),
    tb_noremap: Vec::new(),
    tb_off: 0,
    tb_len: 0,
    tb_maplen: 0,
    tb_silent: 0,
    tb_no_abbr_cnt: 0,
    tb_change_cnt: 0,
});

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

/// Whether there are no characters in the typeahead buffer that have
/// not been typed (result from a mapping or come from `:normal`)
/// (`typebuf_typed`).
#[must_use]
pub fn typebuf_typed() -> bool {
    // SAFETY: momentary read.
    unsafe { TYPEBUF.get_mut() }.tb_maplen == 0
}

/// The number of characters that are mapped (or not typed)
/// (`typebuf_maplen`).
#[must_use]
pub fn typebuf_maplen() -> i32 {
    // SAFETY: momentary read.
    unsafe { TYPEBUF.get_mut() }.tb_maplen
}

/// Whether the typeahead buffer was changed (while waiting for a
/// character to arrive) since `tb_change_cnt` was snapshotted -
/// happens when a message was received from a client or from
/// `feedkeys()` (`typebuf_changed`).
///
/// `tb_change_cnt` is the caller's own OLD value of `typebuf`'s
/// `tb_change_cnt` field (a snapshot taken before waiting).
#[must_use]
pub fn typebuf_changed(tb_change_cnt: i32) -> bool {
    // SAFETY: momentary reads.
    tb_change_cnt != 0
        && (unsafe { TYPEBUF.get_mut() }.tb_change_cnt != tb_change_cnt
            || unsafe { crate::globals::GLOBALS.get_mut() }.typebuf_was_filled)
}

/// Add byte string `s` to `buf` (`add_buff`). Doesn't add empty
/// strings. See this module's own doc comment for why the original's
/// `bh_index`/`bh_space`/`bh_create_newblock` block-management fields
/// have no equivalent here.
fn add_buff(buf: &mut BuffheaderT, s: &[u8]) {
    if s.is_empty() {
        return;
    }
    buf.blocks.push(crate::input_defs::BuffblockT { b_str: s.to_vec() });
}

/// Add number `n` to buffer `buf` (`add_num_buff`).
fn add_num_buff(buf: &mut BuffheaderT, n: i32) {
    add_buff(buf, n.to_string().as_bytes());
}

/// Add byte or special key `c` to buffer `buf`. Translates special
/// keys, NUL and `K_SPECIAL` (`add_byte_buff`).
fn add_byte_buff(buf: &mut BuffheaderT, c: i32) {
    use crate::keycodes_defs::{is_special, k_second, k_third, K_SPECIAL};
    if is_special(c) || c == i32::from(K_SPECIAL) || c == 0 {
        // Translate special key code into three byte sequence.
        add_buff(buf, &[K_SPECIAL, k_second(c), k_third(c)]);
    } else {
        add_buff(buf, &[c as u8]);
    }
}

/// Add character `c` to buffer `buf`. Translates special keys, NUL,
/// `K_SPECIAL` and multibyte characters (`add_char_buff`).
fn add_char_buff(buf: &mut BuffheaderT, c: i32) {
    if crate::keycodes_defs::is_special(c) {
        add_byte_buff(buf, c);
        return;
    }
    let mut bytes = [0u8; crate::mbyte_defs::MB_MAXBYTES + 1];
    let len = crate::mbyte::utf_char2bytes(c, &mut bytes);
    for &b in &bytes[..len as usize] {
        add_byte_buff(buf, i32::from(b));
    }
}

/// Append string `s` to the stuff buffer. `K_SPECIAL` must already
/// have been escaped (`stuffReadbuff`).
pub fn stuff_readbuff(s: &[u8]) {
    // SAFETY: momentary access.
    add_buff(unsafe { READBUF1.get_mut() }, s);
}

/// Append string `s` to the redo stuff buffer. `K_SPECIAL` must
/// already have been escaped (`stuffRedoReadbuff`).
pub fn stuff_redo_readbuff(s: &[u8]) {
    // SAFETY: momentary access.
    add_buff(unsafe { READBUF2.get_mut() }, s);
}

/// Append a character to the stuff buffer. Translates special keys,
/// NUL, `K_SPECIAL` and multibyte characters (`stuffcharReadbuff`).
pub fn stuffchar_readbuff(c: i32) {
    // SAFETY: momentary access.
    add_char_buff(unsafe { READBUF1.get_mut() }, c);
}

/// Append a number to the stuff buffer (`stuffnumReadbuff`).
pub fn stuffnum_readbuff(n: i32) {
    // SAFETY: momentary access.
    add_num_buff(unsafe { READBUF1.get_mut() }, n);
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
            *TYPEBUF.get_mut() = TypebufT::default();
            crate::globals::GLOBALS.get_mut().typebuf_was_filled = false;
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

    #[test]
    fn typebuf_typed_true_by_default() {
        let _lock = global_state_test_lock();
        reset_buffers();
        assert!(typebuf_typed());
        assert_eq!(typebuf_maplen(), 0);
    }

    #[test]
    fn typebuf_typed_false_once_tb_maplen_is_nonzero() {
        let _lock = global_state_test_lock();
        reset_buffers();
        unsafe { TYPEBUF.get_mut() }.tb_maplen = 3;
        assert!(!typebuf_typed());
        assert_eq!(typebuf_maplen(), 3);
        reset_buffers();
    }

    #[test]
    fn typebuf_changed_false_when_snapshot_is_zero() {
        let _lock = global_state_test_lock();
        reset_buffers();
        // The FIRST condition (tb_change_cnt != 0) short-circuits to
        // false regardless of TYPEBUF's own state or typebuf_was_filled.
        unsafe { TYPEBUF.get_mut() }.tb_change_cnt = 5;
        unsafe { crate::globals::GLOBALS.get_mut() }.typebuf_was_filled = true;
        assert!(!typebuf_changed(0));
        reset_buffers();
    }

    #[test]
    fn typebuf_changed_false_when_matching_and_not_filled() {
        let _lock = global_state_test_lock();
        reset_buffers();
        unsafe { TYPEBUF.get_mut() }.tb_change_cnt = 7;
        assert!(!typebuf_changed(7));
        reset_buffers();
    }

    #[test]
    fn typebuf_changed_true_when_snapshot_mismatches() {
        let _lock = global_state_test_lock();
        reset_buffers();
        unsafe { TYPEBUF.get_mut() }.tb_change_cnt = 9;
        assert!(typebuf_changed(2));
        reset_buffers();
    }

    #[test]
    fn typebuf_changed_true_when_matching_but_typebuf_was_filled() {
        let _lock = global_state_test_lock();
        reset_buffers();
        unsafe { TYPEBUF.get_mut() }.tb_change_cnt = 4;
        unsafe { crate::globals::GLOBALS.get_mut() }.typebuf_was_filled = true;
        assert!(typebuf_changed(4));
        reset_buffers();
    }

    // --- add_buff / add_num_buff / add_byte_buff / add_char_buff ---

    #[test]
    fn add_buff_skips_empty_strings() {
        let mut buf = BuffheaderT::default();
        add_buff(&mut buf, b"");
        assert!(buf.blocks.is_empty());
    }

    #[test]
    fn add_buff_pushes_one_block_per_call() {
        let mut buf = BuffheaderT::default();
        add_buff(&mut buf, b"abc");
        add_buff(&mut buf, b"de");
        assert_eq!(buf.blocks.len(), 2);
        assert_eq!(buf.blocks[0].b_str, b"abc");
        assert_eq!(buf.blocks[1].b_str, b"de");
    }

    #[test]
    fn add_num_buff_formats_as_decimal() {
        let mut buf = BuffheaderT::default();
        add_num_buff(&mut buf, 42);
        add_num_buff(&mut buf, -7);
        add_num_buff(&mut buf, 0);
        assert_eq!(buf.blocks[0].b_str, b"42");
        assert_eq!(buf.blocks[1].b_str, b"-7");
        assert_eq!(buf.blocks[2].b_str, b"0");
    }

    #[test]
    fn add_byte_buff_plain_ascii_is_unescaped() {
        let mut buf = BuffheaderT::default();
        add_byte_buff(&mut buf, i32::from(b'a'));
        assert_eq!(buf.blocks.len(), 1);
        assert_eq!(buf.blocks[0].b_str, vec![b'a']);
    }

    #[test]
    fn add_byte_buff_escapes_k_special() {
        let mut buf = BuffheaderT::default();
        add_byte_buff(&mut buf, i32::from(crate::keycodes_defs::K_SPECIAL));
        assert_eq!(buf.blocks.len(), 1);
        assert_eq!(
            buf.blocks[0].b_str,
            vec![crate::keycodes_defs::K_SPECIAL, crate::keycodes_defs::KS_SPECIAL, crate::keycodes_defs::KE_FILLER]
        );
    }

    #[test]
    fn add_byte_buff_escapes_nul() {
        let mut buf = BuffheaderT::default();
        add_byte_buff(&mut buf, 0);
        assert_eq!(buf.blocks.len(), 1);
        assert_eq!(
            buf.blocks[0].b_str,
            vec![crate::keycodes_defs::K_SPECIAL, crate::keycodes_defs::KS_ZERO, crate::keycodes_defs::KE_FILLER]
        );
    }

    #[test]
    fn add_byte_buff_escapes_a_real_special_key() {
        let mut buf = BuffheaderT::default();
        add_byte_buff(&mut buf, crate::keycodes_defs::K_UP);
        assert_eq!(buf.blocks.len(), 1);
        assert_eq!(
            buf.blocks[0].b_str,
            vec![
                crate::keycodes_defs::K_SPECIAL,
                crate::keycodes_defs::key2termcap0(crate::keycodes_defs::K_UP),
                crate::keycodes_defs::key2termcap1(crate::keycodes_defs::K_UP),
            ]
        );
        // K_UP == termcap2key(b'k', b'u'), hand-verified against
        // keycodes_defs.rs's own already-established roundtrip test.
        assert_eq!(buf.blocks[0].b_str, vec![crate::keycodes_defs::K_SPECIAL, b'k', b'u']);
    }

    #[test]
    fn add_char_buff_plain_ascii_produces_one_byte_block() {
        let mut buf = BuffheaderT::default();
        add_char_buff(&mut buf, i32::from(b'z'));
        assert_eq!(buf.blocks.len(), 1);
        assert_eq!(buf.blocks[0].b_str, vec![b'z']);
    }

    #[test]
    fn add_char_buff_nul_is_escaped_as_a_single_3_byte_block() {
        let mut buf = BuffheaderT::default();
        // utf_char2bytes(0, ..) == 1 byte ([0]), which add_byte_buff
        // then escapes as a 3-byte K_SPECIAL/KS_ZERO/KE_FILLER block.
        add_char_buff(&mut buf, 0);
        assert_eq!(buf.blocks.len(), 1);
        assert_eq!(
            buf.blocks[0].b_str,
            vec![crate::keycodes_defs::K_SPECIAL, crate::keycodes_defs::KS_ZERO, crate::keycodes_defs::KE_FILLER]
        );
    }

    #[test]
    fn add_char_buff_u_plus_0080_produces_two_blocks() {
        // U+0080 needs 2 UTF-8 bytes (0xC2 0x80, hand-verified against
        // utf_char2bytes's own 0x80 <= c < 0x800 branch: 0xc0 + (0x80
        // >> 6) = 0xc2, 0x80 + (0x80 & 0x3f) = 0x80). The first byte
        // (0xC2) needs no escaping; the second (0x80) IS K_SPECIAL
        // itself, so it gets its own 3-byte escaped block.
        let mut buf = BuffheaderT::default();
        add_char_buff(&mut buf, 0x80);
        assert_eq!(buf.blocks.len(), 2);
        assert_eq!(buf.blocks[0].b_str, vec![0xC2]);
        assert_eq!(
            buf.blocks[1].b_str,
            vec![crate::keycodes_defs::K_SPECIAL, crate::keycodes_defs::KS_SPECIAL, crate::keycodes_defs::KE_FILLER]
        );
    }

    #[test]
    fn add_char_buff_multibyte_character_with_no_special_bytes() {
        // 'é' (U+00E9) is 2 UTF-8 bytes (0xC3 0xA9), neither of which
        // is 0 or K_SPECIAL (0x80), so each becomes its own plain,
        // unescaped 1-byte block.
        let mut buf = BuffheaderT::default();
        add_char_buff(&mut buf, 0xE9);
        assert_eq!(buf.blocks.len(), 2);
        assert_eq!(buf.blocks[0].b_str, vec![0xC3]);
        assert_eq!(buf.blocks[1].b_str, vec![0xA9]);
    }

    #[test]
    fn add_char_buff_of_a_special_key_skips_utf_char2bytes_entirely() {
        // A real special (negative) key code goes straight to
        // add_byte_buff (len=1, c unchanged) - never treated as a
        // Unicode codepoint via utf_char2bytes.
        let mut buf = BuffheaderT::default();
        add_char_buff(&mut buf, crate::keycodes_defs::K_UP);
        assert_eq!(buf.blocks.len(), 1);
        assert_eq!(buf.blocks[0].b_str, vec![crate::keycodes_defs::K_SPECIAL, b'k', b'u']);
    }

    // --- stuff_readbuff / stuff_redo_readbuff / stuffchar_readbuff / stuffnum_readbuff ---

    #[test]
    fn stuff_readbuff_routes_to_readbuf1() {
        let _lock = global_state_test_lock();
        reset_buffers();
        stuff_readbuff(b"hello");
        assert!(!stuff_empty());
        assert!(!readbuf1_empty());
        assert_eq!(unsafe { READBUF1.get_mut() }.blocks[0].b_str, b"hello");
        assert!(unsafe { READBUF2.get_mut() }.blocks.is_empty());
        reset_buffers();
    }

    #[test]
    fn stuff_readbuff_of_an_empty_string_stays_empty() {
        let _lock = global_state_test_lock();
        reset_buffers();
        stuff_readbuff(b"");
        assert!(stuff_empty());
    }

    #[test]
    fn stuff_redo_readbuff_routes_to_readbuf2() {
        let _lock = global_state_test_lock();
        reset_buffers();
        stuff_redo_readbuff(b"world");
        assert!(!stuff_empty());
        assert!(readbuf1_empty(), "readbuf1 itself is untouched");
        assert_eq!(unsafe { READBUF2.get_mut() }.blocks[0].b_str, b"world");
        reset_buffers();
    }

    #[test]
    fn stuffchar_readbuff_routes_to_readbuf1_via_add_char_buff() {
        let _lock = global_state_test_lock();
        reset_buffers();
        stuffchar_readbuff(i32::from(b'q'));
        assert_eq!(unsafe { READBUF1.get_mut() }.blocks[0].b_str, vec![b'q']);
        reset_buffers();
    }

    #[test]
    fn stuffnum_readbuff_routes_to_readbuf1_via_add_num_buff() {
        let _lock = global_state_test_lock();
        reset_buffers();
        stuffnum_readbuff(123);
        assert_eq!(unsafe { READBUF1.get_mut() }.blocks[0].b_str, b"123");
        reset_buffers();
    }
}
