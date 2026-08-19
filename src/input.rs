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
//! - [`typebuf_typed`]/[`typebuf_maplen`]/[`typebuf_len`] - whether
//!   there are (and how many) untyped (mapped, or from `:normal`)
//!   characters in the typeahead buffer, and how many valid bytes it
//!   currently holds. The new `TYPEBUF: GlobalCell<
//!   crate::input_defs::TypebufT>` instance's own `tb_maplen`/`tb_len`
//!   both start at `0` and are only ever changed inside `ins_typebuf`
//!   (not yet translated), so `typebuf_typed()` is always `true` and
//!   `typebuf_maplen()`/`typebuf_len()` always `0` today.
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
//! Also translated: [`save_typeahead`]/[`restore_typeahead`] - save
//! and restore all 3 kinds of typeahead (the typed-character buffer,
//! plus both stuff/readahead buffers) around a temporary state change
//! (e.g. `:normal`). Needed only already-real pieces:
//! `input_defs.rs`'s own `TasaveT`, this module's own `TYPEBUF`/
//! `READBUF1`/`READBUF2`. `alloc_typebuf`'s own `xmalloc(TYPELEN_INIT)`
//! pre-allocation has no counterpart (nothing to pre-size for an
//! already-owned, on-demand-growing `Vec<u8>`); `free_typebuf`/
//! `free_buff` are real, faithfully-scoped no-ops/near-no-ops (`Vec`'s
//! own `Drop` already does the freeing) - see each function's own doc
//! comment for the exact reasoning, including `free_buff`'s
//! deliberately-preserved `bh_index`-left-untouched quirk. Translated
//! ahead of their real callers (`ex_docmd.c`'s `save_current_state`/
//! `restore_current_state`, and beyond them `exec_normal`/
//! `menu.c`'s `ex_emenu`-adjacent code - none yet translated),
//! matching this crate's established "small, simple, mechanically
//! correct piece ahead of its real caller" precedent.
//!
//! [`stuff_readbuff_len`]/[`stuff_readbuff_spec`]/[`stuffescaped`] are
//! real too. `stuff_readbuff_len` omits only the
//! `add_last_insert == 1` recording side effect: nothing translated
//! can set that register.c-owned flag, so the original's own
//! add-to-readbuf-only path is always taken today.
//!
//! [`ins_typebuf`]/[`del_typebuf`] are real too: Rust's owned vectors
//! replace the
//! original's manual capacity/reallocation branches while preserving
//! the logical offset, valid-byte, remap, mapped/silent and
//! abbreviation counters exactly.
//!
//! Deferred: everything else - `vgetc`/`vgetorpeek` and the whole
//! typeahead-buffer/mapping-application machinery.
//!
//! `redobuff`/`old_redobuff` and their `ResetRedobuff`/
//! `restoreRedobuff`/`AppendToRedobuff`/`AppendCharToRedobuff`/
//! `AppendNumberToRedobuff` group ARE now translated, along with
//! `block_redo`/`typeahead_char` and `typeahead_noflush`. The
//! original's `free_buff` calls before each buffer move are subsumed
//! by assigning an owned value, which drops the previous contents.
//! `CancelRedo` and `saveRedobuff` stay deferred - they need
//! `start_stuff`/`read_readbuffers`/`get_buffcont`. [`stop_redo_ins`],
//! [`can_get_old_char`] and [`may_sync_undo`] are translated too, the
//! second needing a new `old_KeyStuffed` alongside the existing
//! `old_char`.
//!
//! Still deferred: `recordbuff` (macro recording),
//! `close_all_scripts` (real script-file I/O).

use crate::globals::GlobalCell;
use crate::input_defs::{BuffheaderT, TypebufT};

/// First read ahead buffer. Used for translated commands
/// (`readbuf1`). File-static in the original.
static READBUF1: GlobalCell<BuffheaderT> = GlobalCell::new(BuffheaderT { blocks: Vec::new(), bh_index: 0 });

/// Second read ahead buffer. Used for redo (`readbuf2`). File-static
/// in the original.
static READBUF2: GlobalCell<BuffheaderT> = GlobalCell::new(BuffheaderT { blocks: Vec::new(), bh_index: 0 });

/// The redo buffer, holding the last change for the `.` command
/// (`redobuff`). File-static in the original.
static REDOBUFF: GlobalCell<BuffheaderT> = GlobalCell::new(BuffheaderT { blocks: Vec::new(), bh_index: 0 });

/// The previous contents of the redo buffer (`old_redobuff`), kept for
/// the CTRL-O `.` command in Insert mode. File-static in the original.
static OLD_REDOBUFF: GlobalCell<BuffheaderT> = GlobalCell::new(BuffheaderT { blocks: Vec::new(), bh_index: 0 });

/// Iteration state for `read_redo`'s function-local static pointers.
#[derive(Debug, Default)]
struct RedoReaderState {
    bytes: Vec<u8>,
    offset: usize,
}

/// Current redo-buffer reader (`bp`/`p` in `read_redo`).
static REDO_READER: GlobalCell<Option<RedoReaderState>> =
    GlobalCell::new(None);

/// Bytes recorded for the current macro (`recordbuff`).
static RECORDBUFF: GlobalCell<BuffheaderT> =
    GlobalCell::new(BuffheaderT { blocks: Vec::new(), bh_index: 0 });
/// Number of bytes appended by the last input key (`last_recorded_len`).
static LAST_RECORDED_LEN: GlobalCell<usize> = GlobalCell::new(0);

/// Whether appending to the redo buffer is currently suppressed
/// (`block_redo`). File-static in the original.
static BLOCK_REDO: GlobalCell<bool> = GlobalCell::new(false);

/// A typeahead character that will not be flushed (`typeahead_char`).
/// File-static in the original.
static TYPEAHEAD_CHAR: GlobalCell<i32> = GlobalCell::new(0);

/// `getcharmod()` - the modifiers of the last obtained character
/// (`f_getcharmod`, `input.c`).
///
/// # Safety
/// Reads `GLOBALS`.
pub unsafe fn f_getcharmod(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let mod_mask = unsafe { crate::globals::GLOBALS.get_mut() }.mod_mask;
    rettv.value = crate::eval::typval_defs::TypvalValue::Number(i64::from(mod_mask));
}

/// Remove the last `slen` bytes from `buf` (`delete_buff_tail`).
///
/// A no-op when the buffer is empty or its last block is shorter than
/// `slen`, matching the original's own two guards.
///
/// The original edits the current block in place, writing a NUL and
/// adjusting `b_strlen`/`bh_space`; this crate's `Vec<BuffblockT>`
/// representation has neither a length tag nor a capacity tag (see
/// this module's own doc comment), so the equivalent is truncating
/// the last block.
pub fn delete_buff_tail(buf: &mut BuffheaderT, slen: usize) {
    let Some(last) = buf.blocks.last_mut() else {
        return; // nothing to delete
    };
    if last.b_str.len() < slen {
        return;
    }
    let keep = last.b_str.len() - slen;
    last.b_str.truncate(keep);
}

/// Rewind the stuff buffers so their contents are read from the start
/// again (`start_stuff`).
///
/// The original resets each buffer's `bh_curr` to the dummy first
/// block and forces a fresh block on the next append; here the read
/// position is a plain index, so rewinding is setting it back to `0`.
/// Empty buffers are left alone, matching the original's own
/// `bh_first.b_next != NULL` guards.
///
/// # Safety
/// Must not run concurrently with any other access to `READBUF1` or
/// `READBUF2`.
pub unsafe fn start_stuff() {
    // SAFETY: forwarded from this function's own safety doc.
    let rb1 = unsafe { READBUF1.get_mut() };
    if !rb1.blocks.is_empty() {
        rb1.bh_index = 0;
    }
    // SAFETY: as above.
    let rb2 = unsafe { READBUF2.get_mut() };
    if !rb2.blocks.is_empty() {
        rb2.bh_index = 0;
    }
}

/// Reads one byte from a stuff/redo buffer (`read_readbuf`).
///
/// With `advance == false` this peeks. Advancing past a block's last
/// byte removes that block and resets the per-block index.
pub fn read_readbuf(buf: &mut BuffheaderT, advance: bool) -> u8 {
    let Some(first) = buf.blocks.first() else {
        return crate::ascii_defs::NUL;
    };
    let c = first.b_str[buf.bh_index];
    let block_len = first.b_str.len();
    if advance {
        buf.bh_index += 1;
        if buf.bh_index >= block_len {
            buf.blocks.remove(0);
            buf.bh_index = 0;
        }
    }
    c
}

/// Reads one byte from the two stuff buffers (`read_readbuffers`),
/// preferring `READBUF1` and falling back to `READBUF2`.
///
/// `K_SPECIAL` remains escaped; no translation is performed.
pub fn read_readbuffers(advance: bool) -> u8 {
    let c = read_readbuf(unsafe { READBUF1.get_mut() }, advance);
    if c == crate::ascii_defs::NUL {
        read_readbuf(unsafe { READBUF2.get_mut() }, advance)
    } else {
        c
    }
}

/// Concatenates every block in a buffer (`get_buffcont`).
///
/// `dozero` distinguishes the original's allocated empty string from
/// its null result: `Some(Vec::new())` versus `None`.
#[must_use]
pub fn get_buffcont(buf: &BuffheaderT, dozero: bool) -> Option<Vec<u8>> {
    let count: usize = buf.blocks.iter().map(|block| block.b_str.len()).sum();
    if count == 0 && !dozero {
        return None;
    }
    let mut out = Vec::with_capacity(count);
    for block in &buf.blocks {
        out.extend_from_slice(&block.b_str);
    }
    Some(out)
}

/// Return and clear the recorded macro bytes (`get_recorded`).
///
/// # Safety
/// Must not run concurrently with macro recording or writes to
/// `GLOBALS.restart_edit`.
pub unsafe fn get_recorded() -> Vec<u8> {
    let mut recorded =
        get_buffcont(unsafe { RECORDBUFF.get_mut() }, true).unwrap_or_default();
    free_buff(unsafe { RECORDBUFF.get_mut() });

    let last_len = unsafe { *LAST_RECORDED_LEN.get_mut() };
    if recorded.len() >= last_len {
        recorded.truncate(recorded.len() - last_len);
    }
    if !recorded.is_empty()
        && unsafe { crate::globals::GLOBALS.get_mut() }.restart_edit != 0
        && recorded.last() == Some(&crate::ascii_defs::CTRL_O)
    {
        recorded.pop();
    }
    recorded
}

/// Fold a pending CTRL modifier into the character itself where an
/// equivalent control code exists (`merge_modifiers`).
///
/// `@`..`DEL` map onto their control codes; a resulting NUL becomes
/// `K_ZERO`, since NUL cannot travel through the input stream. CTRL-6
/// is special-cased to CTRL-^ so the common keyboard spelling works.
/// The CTRL bit is cleared from `modifiers` only when the character
/// actually changed, so an unaffected key keeps its modifier for a
/// mapping to match on.
#[must_use]
pub fn merge_modifiers(c_arg: i32, modifiers: &mut i32) -> i32 {
    let mut c = c_arg;

    if *modifiers & i32::from(crate::keycodes_defs::MOD_MASK_CTRL) != 0 {
        if (i32::from(b'@')..=0x7f).contains(&c) {
            c &= 0x1f;
            if c == i32::from(crate::ascii_defs::NUL) {
                c = crate::keycodes_defs::K_ZERO;
            }
        } else if c == i32::from(b'6') {
            // CTRL-6 is equivalent to CTRL-^
            c = 0x1e;
        }
        if c != c_arg {
            *modifiers &= !i32::from(crate::keycodes_defs::MOD_MASK_CTRL);
        }
    }

    c
}

/// Set a typeahead character that will not be flushed
/// (`typeahead_noflush`).
///
/// # Safety
/// Must not run concurrently with any other access to `TYPEAHEAD_CHAR`.
pub unsafe fn typeahead_noflush(c: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { TYPEAHEAD_CHAR.get_mut() } = c;
}

/// Start a new redo buffer, keeping the previous one for the CTRL-O
/// `.` command (`ResetRedobuff`).
///
/// Does nothing while redo is blocked, so a caller cannot silently
/// discard a redo buffer another one is still building.
///
/// # Safety
/// Must not run concurrently with any other access to `REDOBUFF`,
/// `OLD_REDOBUFF` or `BLOCK_REDO`.
pub unsafe fn reset_redobuff() {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { *BLOCK_REDO.get_mut() } {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let redo = std::mem::take(unsafe { REDOBUFF.get_mut() });
    // The original frees old_redobuff and then moves redobuff into
    // it; assigning an owned value here frees the old one by dropping.
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { OLD_REDOBUFF.get_mut() } = redo;
}

/// Discard the current redo command and restore the previous one
/// (`CancelRedo`).
///
/// # Safety
/// Must not run concurrently with redo or stuff-buffer access.
pub unsafe fn cancel_redo() {
    if unsafe { *BLOCK_REDO.get_mut() } {
        return;
    }
    let old = std::mem::take(unsafe { OLD_REDOBUFF.get_mut() });
    *unsafe { REDOBUFF.get_mut() } = old;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { start_stuff() };
    while read_readbuffers(true) != crate::ascii_defs::NUL {}
}

/// Save both redo buffers while leaving a copy of the current command
/// available to nested `:normal .` (`saveRedobuff`).
///
/// # Safety
/// Must not run concurrently with redo-buffer access.
pub unsafe fn save_redobuff(save_redo: &mut crate::input_defs::SaveRedoT) {
    save_redo.sr_redobuff = std::mem::take(unsafe { REDOBUFF.get_mut() });
    save_redo.sr_old_redobuff =
        std::mem::take(unsafe { OLD_REDOBUFF.get_mut() });

    let Some(contents) = get_buffcont(&save_redo.sr_redobuff, false) else {
        return;
    };
    add_buff(unsafe { REDOBUFF.get_mut() }, &contents);
}

/// Restore the redo buffers saved by `saveRedobuff`
/// (`restoreRedobuff`), used after running autocommands and user
/// functions.
///
/// # Safety
/// Must not run concurrently with any other access to `REDOBUFF` or
/// `OLD_REDOBUFF`.
pub unsafe fn restore_redobuff(save_redo: &mut crate::input_defs::SaveRedoT) {
    // Both `free_buff` calls in the original are subsumed by
    // assigning an owned value, which drops the previous contents.
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { REDOBUFF.get_mut() } = std::mem::take(&mut save_redo.sr_redobuff);
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { OLD_REDOBUFF.get_mut() } = std::mem::take(&mut save_redo.sr_old_redobuff);
}

/// Append `s` to the redo buffer (`AppendToRedobuff`).
///
/// `K_SPECIAL` should already have been escaped by the caller.
///
/// # Safety
/// Must not run concurrently with any other access to `REDOBUFF` or
/// `BLOCK_REDO`.
pub unsafe fn append_to_redobuff(s: &[u8]) {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { *BLOCK_REDO.get_mut() } {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    add_buff(unsafe { REDOBUFF.get_mut() }, s);
}

/// Append one character to the redo buffer (`AppendCharToRedobuff`),
/// translating special keys, NUL, `K_SPECIAL` and multibyte
/// characters.
///
/// # Safety
/// Must not run concurrently with any other access to `REDOBUFF` or
/// `BLOCK_REDO`.
pub unsafe fn append_char_to_redobuff(c: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { *BLOCK_REDO.get_mut() } {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    add_char_buff(unsafe { REDOBUFF.get_mut() }, c);
}

/// Append a number to the redo buffer (`AppendNumberToRedobuff`).
///
/// # Safety
/// Must not run concurrently with any other access to `REDOBUFF` or
/// `BLOCK_REDO`.
pub unsafe fn append_number_to_redobuff(n: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { *BLOCK_REDO.get_mut() } {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    add_num_buff(unsafe { REDOBUFF.get_mut() }, n);
}

/// Read one character from a redo buffer without changing that buffer
/// (`read_redo`).
///
/// With `init`, prepares a fresh read and returns `OK`/`FAIL`.
#[allow(dead_code)]
unsafe fn read_redo(init: bool, old_redo: bool) -> i32 {
    if init {
        // SAFETY: forwarded from this function's own safety doc.
        let source = if old_redo {
            unsafe { OLD_REDOBUFF.get_mut() }
        } else {
            unsafe { REDOBUFF.get_mut() }
        };
        let bytes: Vec<u8> = source
            .blocks
            .iter()
            .flat_map(|block| block.b_str.iter().copied())
            .collect();
        if bytes.is_empty() {
            return crate::vim_defs::FAIL;
        }
        // SAFETY: forwarded from this function's own safety doc.
        *unsafe { REDO_READER.get_mut() } =
            Some(RedoReaderState { bytes, offset: 0 });
        return crate::vim_defs::OK;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let state = unsafe { REDO_READER.get_mut() }
        .as_mut()
        .expect("read_redo called before initialization");
    let Some(&first) = state.bytes.get(state.offset) else {
        return i32::from(crate::ascii_defs::NUL);
    };
    let mut c = i32::from(first);
    let byte_len = if first != crate::keycodes_defs::K_SPECIAL
        || state.bytes.get(state.offset + 1)
            == Some(&crate::keycodes_defs::KS_SPECIAL)
    {
        crate::mbyte::utf_byte2len(first)
    } else {
        1
    };
    let mut bytes = [0; crate::mbyte_defs::MB_MAXBYTES + 1];

    for index in 0..usize::from(byte_len) {
        if c == i32::from(crate::keycodes_defs::K_SPECIAL) {
            let first = state.bytes[state.offset + 1];
            let second = state.bytes[state.offset + 2];
            c = crate::keycodes_defs::to_special(first, second);
            state.offset += 2;
        }
        state.offset += 1;
        bytes[index] = c as u8;
        if index + 1 == usize::from(byte_len) {
            if byte_len != 1 {
                c = crate::mbyte::utf_ptr2char(&bytes[..usize::from(byte_len)]);
            }
            break;
        }
        c = state
            .bytes
            .get(state.offset)
            .copied()
            .map_or(0, i32::from);
        if c == 0 {
            break;
        }
    }
    c
}

/// Copy the unread redo-buffer characters into the redo stuff buffer
/// (`copy_redo`).
///
/// # Safety
/// `read_redo(true, old_redo)` must have succeeded first.
#[allow(dead_code)]
unsafe fn copy_redo(old_redo: bool) {
    loop {
        // SAFETY: forwarded from this function's own safety doc.
        let c = unsafe { read_redo(false, old_redo) };
        if c == i32::from(crate::ascii_defs::NUL) {
            break;
        }
        // SAFETY: momentary access to the redo stuff buffer.
        add_char_buff(unsafe { READBUF2.get_mut() }, c);
    }
}

/// Stuff a redo command into the redo read buffer (`start_redo`).
///
/// # Safety
/// `GLOBALS.curwin` must be live when the redo starts with Visual-mode
/// marker `v`; otherwise this only accesses serialized input globals.
pub unsafe fn start_redo(count: i32, old_redo: bool) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { read_redo(true, old_redo) } == crate::vim_defs::FAIL {
        return crate::vim_defs::FAIL;
    }
    // SAFETY: reader was initialized above.
    let mut c = unsafe { read_redo(false, old_redo) };

    if c == i32::from(b'"') {
        add_buff(unsafe { READBUF2.get_mut() }, b"\"");
        // SAFETY: reader was initialized above.
        c = unsafe { read_redo(false, old_redo) };
        if (i32::from(b'1')..i32::from(b'9')).contains(&c) {
            c += 1;
        }
        add_char_buff(unsafe { READBUF2.get_mut() }, c);
        if c == i32::from(b'=') {
            add_char_buff(
                unsafe { READBUF2.get_mut() },
                i32::from(crate::ascii_defs::CAR),
            );
            unsafe { crate::globals::GLOBALS.get_mut() }.cmd_silent = true;
        }
        // SAFETY: reader was initialized above.
        c = unsafe { read_redo(false, old_redo) };
    }

    if c == i32::from(b'v') {
        // SAFETY: forwarded from this function's own safety doc.
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        // SAFETY: forwarded from this function's own safety doc.
        globals.Visual.start = unsafe { (*globals.curwin).w_cursor };
        globals.Visual.active = true;
        globals.Visual.select = false;
        globals.Visual.reselect = 1;
        globals.Visual.redo_busy = true;
        // SAFETY: reader was initialized above.
        c = unsafe { read_redo(false, old_redo) };
    }

    if count != 0 {
        while (i32::from(b'0')..=i32::from(b'9')).contains(&c) {
            // SAFETY: reader was initialized above.
            c = unsafe { read_redo(false, old_redo) };
        }
        add_num_buff(unsafe { READBUF2.get_mut() }, count);
    }
    add_char_buff(unsafe { READBUF2.get_mut() }, c);
    // SAFETY: reader was initialized above.
    unsafe { copy_redo(old_redo) };
    crate::vim_defs::OK
}

/// Repeat the text from the last Insert-mode command (`start_redo_ins`).
///
/// # Safety
/// Must not run concurrently with redo or stuff-buffer access.
pub unsafe fn start_redo_ins() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { read_redo(true, false) } == crate::vim_defs::FAIL {
        return crate::vim_defs::FAIL;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { start_stuff() };

    loop {
        // SAFETY: redo reader was initialized above.
        let c = unsafe { read_redo(false, false) };
        if c == i32::from(crate::ascii_defs::NUL) {
            break;
        }
        if crate::strings::vim_strchr(b"AaIiRrOo", c).is_some() {
            if c == i32::from(b'O') || c == i32::from(b'o') {
                add_buff(unsafe { READBUF2.get_mut() }, b"\n");
            }
            break;
        }
    }

    // SAFETY: redo reader was initialized above.
    unsafe { copy_redo(false) };
    *unsafe { BLOCK_REDO.get_mut() } = true;
    crate::vim_defs::OK
}

/// Returns the redo buffer as one owned byte string (`get_inserted`).
///
/// `None` preserves the original null `String.data` for an empty
/// buffer; escaped `K_SPECIAL` bytes are returned unchanged.
#[must_use]
pub fn get_inserted() -> Option<Vec<u8>> {
    get_buffcont(unsafe { REDOBUFF.get_mut() }, false)
}

/// Index in `scriptin` (`curscript`). File-static in the original;
/// `-1` means no script is being read.
static CURSCRIPT: GlobalCell<i32> = GlobalCell::new(-1);

/// Maximum nested script input streams (`NSCRIPT`).
pub const NSCRIPT: usize = 15;

/// Typeahead saved for each active `:source!` stream (`saved_typebuf`).
#[allow(dead_code)]
static SAVED_TYPEBUF: std::sync::LazyLock<
    GlobalCell<[TypebufT; NSCRIPT]>,
> = std::sync::LazyLock::new(|| {
    GlobalCell::new(std::array::from_fn(|_| TypebufT::default()))
});

/// State for assembling one recorded or show-command key
/// (`gotchars_state_T`).
#[allow(dead_code)]
#[derive(Debug)]
struct GotcharsStateT {
    buf: [u8; crate::mbyte_defs::MB_MAXBYTES * 3 + 4],
    prev_c: i32,
    buflen: usize,
    pending_special: u32,
    pending_mbyte: u32,
}

impl Default for GotcharsStateT {
    fn default() -> Self {
        Self {
            buf: [0; crate::mbyte_defs::MB_MAXBYTES * 3 + 4],
            prev_c: 0,
            buflen: 0,
            pending_special: 0,
            pending_mbyte: 0,
        }
    }
}

/// Add one byte to a recording/show-command key assembler
/// (`gotchars_add_byte`).
#[allow(dead_code)]
fn gotchars_add_byte(state: &mut GotcharsStateT, byte: u8) -> bool {
    state.buf[state.buflen] = byte;
    state.buflen += 1;
    let mut c = i32::from(byte);
    let in_special = state.pending_special > 0;
    let in_mbyte = state.pending_mbyte > 0;

    if in_special {
        state.pending_special -= 1;
    } else if byte == crate::keycodes_defs::K_SPECIAL {
        state.pending_special = 2;
    }
    if state.pending_special > 0 {
        state.prev_c = c;
        return false;
    }

    if in_mbyte {
        state.pending_mbyte -= 1;
    } else {
        if in_special {
            if state.prev_c == i32::from(crate::keycodes_defs::KS_MODIFIER) {
                state.prev_c = c;
                return false;
            }
            c = crate::keycodes_defs::termcap2key(state.prev_c as u8, byte);
            if c == crate::keycodes_defs::K_COMPLETE_DELAY {
                state.buflen = 0;
            }
        }
        let byte_len = if (0..=255).contains(&c) {
            crate::mbyte::utf_byte2len(c as u8)
        } else {
            1
        };
        state.pending_mbyte = u32::from(byte_len.saturating_sub(1));
    }

    state.prev_c = c;
    state.pending_mbyte == 0
}

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
/// `tb_noremap`: allow abbreviations but no mapping (`RM_ABBR`).
pub const RM_ABBR: i32 = 4;
/// `tb_noremap`: allow normal remapping (`RM_YES`).
pub const RM_YES: i32 = 0;

/// Character put back by `vungetc()` (`old_char`), file-static in the
/// original. Starts at `-1` ("none put back"), matching the original's
/// own `static int old_char = -1;`.
static OLD_CHAR: GlobalCell<i32> = GlobalCell::new(-1);

/// `mod_mask` for [`OLD_CHAR`] (`old_mod_mask`), file-static in the
/// original. Starts at `0`, matching the original's own
/// zero-initialized `static int old_mod_mask;`.
static OLD_MOD_MASK: GlobalCell<i32> = GlobalCell::new(0);
static OLD_MOUSE_GRID: GlobalCell<i32> = GlobalCell::new(0);
static OLD_MOUSE_ROW: GlobalCell<i32> = GlobalCell::new(0);
static OLD_MOUSE_COL: GlobalCell<i32> = GlobalCell::new(0);

/// Whether [`OLD_CHAR`] was stuffed rather than typed
/// (`old_KeyStuffed`), file-static in the original.
static OLD_KEY_STUFFED: GlobalCell<bool> = GlobalCell::new(false);

/// Puts one consumed character back for the next `vgetc()` call
/// (`vungetc`), together with the input metadata that belonged to it.
///
/// # Safety
/// Mutates the `OLD_*` file-statics and reads the current input fields
/// from `GLOBALS`.
pub unsafe fn vungetc(c: i32) {
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    unsafe {
        *OLD_CHAR.get_mut() = c;
        *OLD_MOD_MASK.get_mut() = globals.mod_mask;
        *OLD_MOUSE_GRID.get_mut() = globals.mouse_grid;
        *OLD_MOUSE_ROW.get_mut() = globals.mouse_row;
        *OLD_MOUSE_COL.get_mut() = globals.mouse_col;
        *OLD_KEY_STUFFED.get_mut() = globals.KeyStuffed != 0;
    }
}

/// Sync undo before a blocking wait, unless it would break an
/// in-progress edit (`may_sync_undo`).
///
/// Skipped in Insert or Cmdline mode unless a cursor key has been
/// used (which already ends the current undo block), and skipped
/// entirely while reading a script file.
///
/// # Safety
/// Must not run concurrently with any other access to
/// `crate::globals::GLOBALS` or `CURSCRIPT`, and forwarded from
/// [`crate::undo::u_sync`].
pub unsafe fn may_sync_undo() {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    let in_insert_or_cmdline = globals.State as u32
        & (crate::state_defs::mode::INSERT | crate::state_defs::mode::CMDLINE)
        != 0;
    let arrow_used = globals.Ins.arrow_used;
    // SAFETY: forwarded from this function's own safety doc.
    let curscript = unsafe { *CURSCRIPT.get_mut() };

    if (!in_insert_or_cmdline || arrow_used) && curscript < 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::undo::u_sync(false) };
    }
}

/// Whether the character put back by `vungetc()` can be taken now
/// (`can_get_old_char`).
///
/// If that character was NOT stuffed and something has since been
/// added to the stuff buffer, the stuffed characters have to be
/// consumed first - so a put-back character alone is not enough.
///
/// # Safety
/// Must not run concurrently with any write to `OLD_CHAR`,
/// `OLD_KEY_STUFFED`, `READBUF1` or `READBUF2`.
#[must_use]
pub unsafe fn can_get_old_char() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let old_char = unsafe { *OLD_CHAR.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let stuffed = unsafe { *OLD_KEY_STUFFED.get_mut() };
    old_char != -1 && (stuffed || stuff_empty())
}

/// Defers or completes clearing `reg_executing` after a peek
/// (`check_end_reg_executing`).
///
/// # Safety
/// Mutates `GLOBALS.reg_executing`/
/// `pending_end_reg_executing` and reads `TYPEBUF`.
pub unsafe fn check_end_reg_executing(advance: bool) {
    let maplen = unsafe { TYPEBUF.get_mut() }.tb_maplen;
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    if globals.reg_executing != 0
        && (maplen == 0 || globals.pending_end_reg_executing)
    {
        if advance {
            globals.reg_executing = 0;
            globals.pending_end_reg_executing = false;
        } else {
            globals.pending_end_reg_executing = true;
        }
    }
}

/// Stop blocking the redo buffer after an Insert-mode redo
/// (`stop_redo_ins`).
///
/// # Safety
/// Must not run concurrently with any other access to `BLOCK_REDO`.
pub unsafe fn stop_redo_ins() {
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { BLOCK_REDO.get_mut() } = false;
}

/// Maximum length of a key sequence to be mapped (`MAXMAPLEN`,
/// `mapping_defs.h`). Defined here (rather than a dedicated
/// `mapping_defs.rs`, which doesn't exist yet) since [`alloc_typebuf`]
/// is currently its only real user in this crate.
const MAXMAPLEN: i32 = 50;

/// Initializes the static emergency typeahead buffer
/// (`init_typebuf`) once.
///
/// In the C representation a null `tb_buf` marks "not initialized".
/// In the Vec representation `tb_change_cnt == 0` is the equivalent
/// sentinel: every real allocation sets it nonzero and preserves that
/// invariant across wraparound.
pub fn init_typebuf() {
    let tb = unsafe { TYPEBUF.get_mut() };
    if tb.tb_change_cnt != 0 {
        return;
    }
    tb.tb_buf.clear();
    tb.tb_noremap.clear();
    tb.tb_off = MAXMAPLEN + 4;
    tb.tb_len = 0;
    tb.tb_maplen = 0;
    tb.tb_silent = 0;
    tb.tb_no_abbr_cnt = 0;
    tb.tb_change_cnt = 1;
}

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

/// The number of valid bytes currently in the typeahead buffer
/// (`typebuf.tb_len`, read directly in the original - not a named
/// function there, but exposed as one here since `TYPEBUF` itself is
/// private to this module, matching `typebuf_maplen`'s own shape).
#[must_use]
pub fn typebuf_len() -> i32 {
    // SAFETY: momentary read.
    unsafe { TYPEBUF.get_mut() }.tb_len
}

#[cfg(test)]
pub(crate) fn typebuf_bytes_for_test() -> Vec<u8> {
    let tb = unsafe { TYPEBUF.get_mut() };
    let start = tb.tb_off as usize;
    let end = start + tb.tb_len as usize;
    tb.tb_buf[start..end].to_vec()
}

#[cfg(test)]
pub(crate) fn typebuf_remap_for_test() -> Vec<u8> {
    let tb = unsafe { TYPEBUF.get_mut() };
    let start = tb.tb_off as usize;
    let end = start + tb.tb_len as usize;
    tb.tb_noremap[start..end].to_vec()
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

/// Insert bytes at `offset` in the typeahead buffer (`ins_typebuf`).
///
/// `noremap` accepts a [`crate::input_defs::RemapValues`] discriminant
/// or a positive count of leading bytes that cannot be remapped.
/// Input ends at the first NUL, matching the original's C-string
/// contract.
///
/// # Panics
/// Debug-asserts that `offset` is within the current valid byte range,
/// matching the original's caller precondition.
pub fn ins_typebuf(
    text: &[u8],
    noremap: i32,
    offset: i32,
    nottyped: bool,
    silent: bool,
) -> i32 {
    init_typebuf();
    let tb = unsafe { TYPEBUF.get_mut() };
    tb.tb_change_cnt = tb.tb_change_cnt.wrapping_add(1);
    if tb.tb_change_cnt == 0 {
        tb.tb_change_cnt = 1;
    }
    crate::state::state_no_longer_safe();

    let addlen = text
        .iter()
        .position(|&byte| byte == crate::ascii_defs::NUL)
        .unwrap_or(text.len());
    let text = &text[..addlen];
    let old_len = usize::try_from(tb.tb_len).expect("typebuf length is nonnegative");
    let old_off = usize::try_from(tb.tb_off).expect("typebuf offset is nonnegative");
    let offset = usize::try_from(offset).expect("typebuf insertion offset is nonnegative");
    debug_assert!(offset <= old_len);
    if offset > old_len {
        return crate::vim_defs::FAIL;
    }
    if addlen > i32::MAX as usize
        || old_len > i32::MAX as usize - addlen
    {
        return crate::vim_defs::FAIL;
    }

    let old_bytes = if old_len == 0 {
        Vec::new()
    } else {
        debug_assert!(old_off + old_len <= tb.tb_buf.len());
        tb.tb_buf[old_off..old_off + old_len].to_vec()
    };
    let old_noremap = if old_len == 0 {
        Vec::new()
    } else {
        debug_assert!(old_off + old_len <= tb.tb_noremap.len());
        tb.tb_noremap[old_off..old_off + old_len].to_vec()
    };

    let new_off = if offset == 0 && addlen <= old_off {
        old_off - addlen
    } else {
        MAXMAPLEN as usize + 4
    };
    let new_len = old_len + addlen;
    let mut bytes = Vec::with_capacity(new_off + new_len + 1);
    bytes.resize(new_off, 0);
    bytes.extend_from_slice(&old_bytes[..offset]);
    bytes.extend_from_slice(text);
    bytes.extend_from_slice(&old_bytes[offset..]);
    bytes.push(0);

    let mut remap = Vec::with_capacity(new_off + new_len);
    remap.resize(new_off, 0);
    remap.extend_from_slice(&old_noremap[..offset]);
    remap.resize(remap.len() + addlen, RM_YES as u8);
    remap.extend_from_slice(&old_noremap[offset..]);

    let value = if noremap == crate::input_defs::RemapValues::Script as i32 {
        RM_SCRIPT
    } else if noremap == crate::input_defs::RemapValues::Skip as i32 {
        RM_ABBR
    } else {
        RM_NONE
    };
    let mut remaining = if noremap == crate::input_defs::RemapValues::Skip as i32 {
        1
    } else if noremap < 0 {
        i32::try_from(addlen).expect("typebuf insertion length fits i32")
    } else {
        noremap
    };
    for flag in &mut remap[new_off + offset..new_off + offset + addlen] {
        remaining -= 1;
        *flag = if remaining >= 0 {
            value as u8
        } else {
            RM_YES as u8
        };
    }

    let offset_i32 = i32::try_from(offset).expect("typebuf insertion offset fits i32");
    let addlen_i32 = i32::try_from(addlen).expect("typebuf insertion length fits i32");
    if nottyped || tb.tb_maplen > offset_i32 {
        tb.tb_maplen += addlen_i32;
    }
    if silent || tb.tb_silent > offset_i32 {
        tb.tb_silent += addlen_i32;
        unsafe { crate::globals::GLOBALS.get_mut() }.cmd_silent = true;
    }
    if tb.tb_no_abbr_cnt != 0 && offset == 0 {
        tb.tb_no_abbr_cnt += addlen_i32;
    }

    tb.tb_buf = bytes;
    tb.tb_noremap = remap;
    tb.tb_off = i32::try_from(new_off).expect("typebuf offset fits i32");
    tb.tb_len = i32::try_from(new_len).expect("typebuf length fits i32");
    crate::vim_defs::OK
}

/// Remove `len` bytes at `offset` from the typeahead buffer
/// (`del_typebuf`).
///
/// The original's choice between advancing `tb_off` and moving bytes
/// is a raw-buffer capacity optimization. This translation advances
/// `tb_off` for a front deletion and uses `Vec::drain` otherwise,
/// preserving all observable bytes, flags and counters.
///
/// # Panics
/// Debug-asserts that the removal range lies inside the valid bytes,
/// matching the original's caller precondition.
pub fn del_typebuf(len: i32, offset: i32) {
    if len == 0 {
        return;
    }
    let len = usize::try_from(len).expect("typebuf deletion length is nonnegative");
    let offset =
        usize::try_from(offset).expect("typebuf deletion offset is nonnegative");
    let tb = unsafe { TYPEBUF.get_mut() };
    let old_len = usize::try_from(tb.tb_len).expect("typebuf length is nonnegative");
    let old_off = usize::try_from(tb.tb_off).expect("typebuf offset is nonnegative");
    debug_assert!(offset + len <= old_len);
    if offset + len > old_len {
        return;
    }

    tb.tb_len -= i32::try_from(len).expect("typebuf deletion length fits i32");
    if offset == 0 {
        tb.tb_off += i32::try_from(len).expect("typebuf deletion length fits i32");
    } else {
        let start = old_off + offset;
        tb.tb_buf.drain(start..start + len);
        tb.tb_noremap.drain(start..start + len);
    }

    let offset = i32::try_from(offset).expect("typebuf deletion offset fits i32");
    let len = i32::try_from(len).expect("typebuf deletion length fits i32");
    for count in [
        &mut tb.tb_maplen,
        &mut tb.tb_silent,
        &mut tb.tb_no_abbr_cnt,
    ] {
        if *count > offset {
            if *count < offset + len {
                *count = offset;
            } else {
                *count -= len;
            }
        }
    }

    unsafe { crate::globals::GLOBALS.get_mut() }.typebuf_was_filled = false;
    tb.tb_change_cnt = tb.tb_change_cnt.wrapping_add(1);
    if tb.tb_change_cnt == 0 {
        tb.tb_change_cnt = 1;
    }
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

/// Append an explicitly bounded string to the stuff buffer
/// (`stuffReadbuffLen`).
///
/// The Rust slice already carries the original's separate `len`
/// argument. The `add_last_insert == 1` side effect remains
/// unreachable; see this module's own doc comment.
pub fn stuff_readbuff_len(s: &[u8]) {
    // SAFETY: momentary access.
    add_buff(unsafe { READBUF1.get_mut() }, s);
}

/// Stuff text while preserving complete special-key triplets and
/// replacing CR, NL and ESC with spaces (`stuffReadbuffSpec`).
pub fn stuff_readbuff_spec(s: &[u8]) {
    let end = s
        .iter()
        .position(|&byte| byte == crate::ascii_defs::NUL)
        .unwrap_or(s.len());
    let mut offset = 0;
    while offset < end {
        if s[offset] == crate::keycodes_defs::K_SPECIAL
            && offset + 2 < end
        {
            stuff_readbuff_len(&s[offset..offset + 3]);
            offset += 3;
            continue;
        }

        let (mut c, consumed) =
            crate::mbyte::mb_cptr2char_adv(&s[offset..end]);
        offset += consumed.max(1);
        if matches!(
            c,
            value if value == i32::from(crate::ascii_defs::CAR)
                || value == i32::from(crate::ascii_defs::NL)
                || value == i32::from(crate::ascii_defs::ESC)
        ) {
            c = i32::from(b' ');
        }
        stuffchar_readbuff(c);
    }
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

/// Stuff text literally or as interpreted key bytes (`stuffescaped`).
pub fn stuffescaped(argument: &[u8], literally: bool) {
    let end = argument
        .iter()
        .position(|&byte| byte == crate::ascii_defs::NUL)
        .unwrap_or(argument.len());
    let mut offset = 0;
    while offset < end {
        let start = offset;
        while offset < end
            && ((b' '..crate::ascii_defs::DEL).contains(&argument[offset])
                || (argument[offset] == crate::keycodes_defs::K_SPECIAL
                    && !literally))
        {
            offset += 1;
        }
        if offset > start {
            stuff_readbuff_len(&argument[start..offset]);
        }
        if offset < end {
            let c = crate::mbyte::utf_ptr2char(&argument[offset..end]);
            offset += crate::mbyte::utf_ptr2len(&argument[offset..end]).max(1) as usize;
            if literally
                && ((c < i32::from(b' ') && c != i32::from(crate::ascii_defs::TAB))
                    || c == i32::from(crate::ascii_defs::DEL))
            {
                stuffchar_readbuff(i32::from(crate::ascii_defs::CTRL_V));
            }
            stuffchar_readbuff(c);
        }
    }
}

/// Free and clear a buffer (`free_buff`).
///
/// Only clears `buf.blocks` - matching the original's own real quirk
/// of leaving `bh_index` untouched (it frees/clears the block list and
/// resets `bh_curr` to `NULL`, but never resets `bh_index`); this
/// crate's own [`BuffheaderT`] redesign has no `bh_curr`/`bh_space`/
/// `bh_create_newblock` counterpart at all (see that struct's own doc
/// comment), so clearing `blocks` is the only real remaining effect.
fn free_buff(buf: &mut BuffheaderT) {
    buf.blocks.clear();
}

/// Make [`TYPEBUF`] empty and allocate new buffers (`alloc_typebuf`).
///
/// The original's own `xmalloc(TYPELEN_INIT)` pre-allocations for
/// `tb_buf`/`tb_noremap` (raw buffers with a fixed initial capacity,
/// `TYPELEN_INIT == 5 * (MAXMAPLEN + 3)` bytes) have no counterpart
/// here: both fields are already owned `Vec<u8>`s (see
/// `input_defs.rs`'s own `TypebufT` doc comment) that grow on demand,
/// so there is nothing to pre-size.
fn alloc_typebuf() {
    let tb = unsafe { TYPEBUF.get_mut() };
    tb.tb_buf = Vec::new();
    tb.tb_noremap = Vec::new();
    tb.tb_off = MAXMAPLEN + 4;
    tb.tb_len = 0;
    tb.tb_maplen = 0;
    tb.tb_silent = 0;
    tb.tb_no_abbr_cnt = 0;
    tb.tb_change_cnt = tb.tb_change_cnt.wrapping_add(1);
    if tb.tb_change_cnt == 0 {
        tb.tb_change_cnt = 1;
    }
    unsafe { crate::globals::GLOBALS.get_mut() }.typebuf_was_filled = false;
}

/// Save the current typeahead for the active script (`save_typebuf`).
#[allow(dead_code)]
fn save_typebuf() {
    let script = unsafe { *CURSCRIPT.get_mut() };
    assert!(script >= 0);
    init_typebuf();
    (unsafe { SAVED_TYPEBUF.get_mut() })[script as usize] =
        unsafe { TYPEBUF.get_mut() }.clone();
    alloc_typebuf();
}

/// Free the buffers of [`TYPEBUF`] (`free_typebuf`).
///
/// A real no-op here: `Vec<u8>`'s own `Drop` already frees `tb_buf`'s/
/// `tb_noremap`'s storage whenever they are overwritten or the struct
/// itself is dropped, so there is nothing left to do once
/// [`save_typeahead`] has already cloned the buffer contents it needs
/// to preserve. The original's own `internal_error` sanity checks
/// (guarding against double-freeing the still-in-use static initial
/// buffers `typebuf_init`/`noremapbuf_init`) have no counterpart
/// either: those buffers don't exist in this crate's `Vec`-based
/// redesign in the first place. Kept as a real, named, callable
/// function (rather than inlined away) since the original has a
/// second call site, in the not-yet-translated `closescript`.
fn free_typebuf() {}

/// Save all three kinds of typeahead, so that the user must type at a
/// prompt (`save_typeahead`).
///
/// Clones `TYPEBUF`'s current contents into `tp.save_typebuf` (the
/// original's own `tp->save_typebuf = typebuf;` plain struct
/// assignment - a full deep copy here rather than the original's
/// "steal the pointer" trick, since `tb_buf`/`tb_noremap` are owned
/// `Vec<u8>`s, not raw pointers; the momentary extra copy is
/// immediately made moot by `alloc_typebuf`'s own very next
/// overwrite of the live buffers, so this has no observable effect on
/// any caller), then resets the live typeahead buffer via
/// `alloc_typebuf` - called in that exact order so `tb_change_cnt`
/// continues counting up from its real, pre-save value (matching the
/// original's own "copy first, increment the still-untouched original
/// second" order precisely).
///
/// Always sets `tp.typebuf_valid = true`, matching the original
/// exactly (there is no failure path in the current implementation).
pub fn save_typeahead(tp: &mut crate::input_defs::TasaveT) {
    tp.save_typebuf = unsafe { TYPEBUF.get_mut() }.clone();
    alloc_typebuf();
    tp.typebuf_valid = true;

    tp.old_char = unsafe { *OLD_CHAR.get_mut() };
    tp.old_mod_mask = unsafe { *OLD_MOD_MASK.get_mut() };
    unsafe { *OLD_CHAR.get_mut() = -1 };

    tp.save_readbuf1 = unsafe { READBUF1.get_mut() }.clone();
    free_buff(unsafe { READBUF1.get_mut() });
    tp.save_readbuf2 = unsafe { READBUF2.get_mut() }.clone();
    free_buff(unsafe { READBUF2.get_mut() });
}

/// Restore the typeahead to what it was before calling
/// [`save_typeahead`] (`restore_typeahead`).
///
/// Should only be called once per [`save_typeahead`] call (matching
/// the original's own "can only be called once!" doc comment) - a
/// second call would restore whatever `tp`'s fields happen to still
/// hold (this crate does not statically enforce single-use, matching
/// the original's own lack of enforcement).
pub fn restore_typeahead(tp: &mut crate::input_defs::TasaveT) {
    if tp.typebuf_valid {
        free_typebuf();
        *unsafe { TYPEBUF.get_mut() } = std::mem::take(&mut tp.save_typebuf);
    }

    unsafe { *OLD_CHAR.get_mut() = tp.old_char };
    unsafe { *OLD_MOD_MASK.get_mut() = tp.old_mod_mask };

    free_buff(unsafe { READBUF1.get_mut() });
    *unsafe { READBUF1.get_mut() } = std::mem::take(&mut tp.save_readbuf1);
    free_buff(unsafe { READBUF2.get_mut() });
    *unsafe { READBUF2.get_mut() } = std::mem::take(&mut tp.save_readbuf2);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::global_state_test_lock;
    use crate::input_defs::BuffblockT;

    // --- f_getcharmod ---

    #[test]
    fn getcharmod_reports_the_current_modifier_mask() {
        let _lock = global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.mod_mask;
        g.mod_mask = 0x24;

        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        unsafe { f_getcharmod(&[], &mut rettv) };

        unsafe { crate::globals::GLOBALS.get_mut() }.mod_mask = prev;
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(0x24));
    }

    #[test]
    fn getcharmod_reports_zero_when_no_modifiers_are_held() {
        let _lock = global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.mod_mask;
        g.mod_mask = 0;

        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        unsafe { f_getcharmod(&[], &mut rettv) };

        unsafe { crate::globals::GLOBALS.get_mut() }.mod_mask = prev;
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(0));
    }

    // --- delete_buff_tail / start_stuff ---

    #[test]
    fn delete_buff_tail_trims_the_last_block() {
        let mut buf = BuffheaderT {
            blocks: vec![
                BuffblockT { b_str: b"keep".to_vec() },
                BuffblockT { b_str: b"abcdef".to_vec() },
            ],
            bh_index: 0,
        };
        delete_buff_tail(&mut buf, 2);
        assert_eq!(buf.blocks[1].b_str, b"abcd".to_vec());
        assert_eq!(buf.blocks[0].b_str, b"keep".to_vec(), "earlier blocks untouched");
    }

    #[test]
    fn delete_buff_tail_can_empty_the_last_block_exactly() {
        let mut buf = BuffheaderT {
            blocks: vec![BuffblockT { b_str: b"abc".to_vec() }],
            bh_index: 0,
        };
        delete_buff_tail(&mut buf, 3);
        assert!(buf.blocks[0].b_str.is_empty());
    }

    #[test]
    fn delete_buff_tail_is_a_noop_when_it_would_overrun() {
        // Matches the original's own `b_strlen < slen` guard: the
        // block is left completely alone rather than partly trimmed.
        let mut buf = BuffheaderT {
            blocks: vec![BuffblockT { b_str: b"ab".to_vec() }],
            bh_index: 0,
        };
        delete_buff_tail(&mut buf, 5);
        assert_eq!(buf.blocks[0].b_str, b"ab".to_vec());
    }

    #[test]
    fn delete_buff_tail_is_a_noop_on_an_empty_buffer() {
        let mut buf = BuffheaderT::default();
        delete_buff_tail(&mut buf, 1);
        assert!(buf.blocks.is_empty());
    }

    #[test]
    fn start_stuff_rewinds_only_non_empty_buffers() {
        let _lock = global_state_test_lock();
        unsafe {
            let rb1 = READBUF1.get_mut();
            let rb2 = READBUF2.get_mut();
            let (save1, save2) = (rb1.clone(), rb2.clone());

            rb1.blocks = vec![BuffblockT { b_str: b"x".to_vec() }];
            rb1.bh_index = 4;
            // Left empty, so its index must survive untouched.
            rb2.blocks.clear();
            rb2.bh_index = 9;

            start_stuff();

            assert_eq!(READBUF1.get_mut().bh_index, 0, "a non-empty buffer rewinds");
            assert_eq!(READBUF2.get_mut().bh_index, 9, "an empty buffer is left alone");

            *READBUF1.get_mut() = save1;
            *READBUF2.get_mut() = save2;
        }
    }

    #[test]
    fn read_readbuf_peeks_without_advancing() {
        let mut buf = BuffheaderT {
            blocks: vec![BuffblockT {
                b_str: b"ab".to_vec(),
            }],
            ..Default::default()
        };

        assert_eq!(read_readbuf(&mut buf, false), b'a');
        assert_eq!(read_readbuf(&mut buf, false), b'a');
        assert_eq!(buf.bh_index, 0);
    }

    #[test]
    fn read_readbuf_advances_and_removes_a_finished_block() {
        let mut buf = BuffheaderT {
            blocks: vec![
                BuffblockT {
                    b_str: b"ab".to_vec(),
                },
                BuffblockT {
                    b_str: b"c".to_vec(),
                },
            ],
            ..Default::default()
        };

        assert_eq!(read_readbuf(&mut buf, true), b'a');
        assert_eq!(buf.bh_index, 1);
        assert_eq!(read_readbuf(&mut buf, true), b'b');
        assert_eq!(buf.bh_index, 0);
        assert_eq!(buf.blocks.len(), 1);
        assert_eq!(read_readbuf(&mut buf, true), b'c');
        assert!(buf.blocks.is_empty());
    }

    #[test]
    fn read_readbuf_returns_nul_for_an_empty_buffer() {
        assert_eq!(
            read_readbuf(&mut BuffheaderT::default(), true),
            crate::ascii_defs::NUL
        );
    }

    #[test]
    fn read_readbuffers_prefers_the_first_buffer() {
        let _lock = global_state_test_lock();
        let _g = OldCharGuard::new();
        unsafe {
            READBUF1.get_mut().blocks.push(BuffblockT {
                b_str: b"a".to_vec(),
            });
            READBUF2.get_mut().blocks.push(BuffblockT {
                b_str: b"b".to_vec(),
            });
        }

        assert_eq!(read_readbuffers(true), b'a');
        assert_eq!(unsafe { READBUF2.get_mut().bh_index }, 0);
    }

    #[test]
    fn read_readbuffers_falls_back_to_the_second_buffer() {
        let _lock = global_state_test_lock();
        let _g = OldCharGuard::new();
        unsafe {
            READBUF2.get_mut().blocks.push(BuffblockT {
                b_str: b"b".to_vec(),
            });
        }

        assert_eq!(read_readbuffers(true), b'b');
        assert!(unsafe { READBUF2.get_mut().blocks.is_empty() });
    }

    #[test]
    fn read_readbuffers_peeks_and_reports_nul_when_both_are_empty() {
        let _lock = global_state_test_lock();
        let _g = OldCharGuard::new();
        assert_eq!(read_readbuffers(true), crate::ascii_defs::NUL);
        unsafe {
            READBUF1.get_mut().blocks.push(BuffblockT {
                b_str: b"x".to_vec(),
            });
        }
        assert_eq!(read_readbuffers(false), b'x');
        assert_eq!(read_readbuffers(false), b'x');
    }

    #[test]
    fn get_buffcont_concatenates_blocks_without_consuming_them() {
        let buf = BuffheaderT {
            blocks: vec![
                BuffblockT {
                    b_str: b"ab".to_vec(),
                },
                BuffblockT {
                    b_str: b"cd".to_vec(),
                },
            ],
            ..Default::default()
        };

        assert_eq!(get_buffcont(&buf, false), Some(b"abcd".to_vec()));
        assert_eq!(buf.blocks.len(), 2);
    }

    #[test]
    fn get_buffcont_distinguishes_null_from_allocated_empty() {
        let buf = BuffheaderT::default();
        assert_eq!(get_buffcont(&buf, false), None);
        assert_eq!(get_buffcont(&buf, true), Some(Vec::new()));
    }

    #[test]
    fn get_buffcont_preserves_escaped_special_key_bytes() {
        let buf = BuffheaderT {
            blocks: vec![BuffblockT {
                b_str: vec![crate::keycodes_defs::K_SPECIAL, 0x12, 0x34],
            }],
            ..Default::default()
        };
        assert_eq!(
            get_buffcont(&buf, false),
            Some(vec![crate::keycodes_defs::K_SPECIAL, 0x12, 0x34])
        );
    }

    #[test]
    fn get_recorded_removes_the_stopping_key_and_clears_recording_storage() {
        let _lock = global_state_test_lock();
        let saved = std::mem::take(unsafe { RECORDBUFF.get_mut() });
        let saved_len = std::mem::replace(
            unsafe { LAST_RECORDED_LEN.get_mut() },
            2,
        );
        unsafe {
            add_buff(RECORDBUFF.get_mut(), b"macro");
            add_buff(RECORDBUFF.get_mut(), b"q!");
        }

        assert_eq!(unsafe { get_recorded() }, b"macro".to_vec());
        assert!(unsafe { RECORDBUFF.get_mut() }.blocks.is_empty());

        *unsafe { RECORDBUFF.get_mut() } = saved;
        *unsafe { LAST_RECORDED_LEN.get_mut() } = saved_len;
    }

    #[test]
    fn get_recorded_removes_ctrl_o_when_stopping_from_insert_mode() {
        let _lock = global_state_test_lock();
        let saved = std::mem::take(unsafe { RECORDBUFF.get_mut() });
        let saved_len = std::mem::replace(
            unsafe { LAST_RECORDED_LEN.get_mut() },
            1,
        );
        let restart = std::mem::replace(
            &mut unsafe { crate::globals::GLOBALS.get_mut() }.restart_edit,
            1,
        );
        unsafe { add_buff(RECORDBUFF.get_mut(), b"text\x0fq") };

        assert_eq!(unsafe { get_recorded() }, b"text".to_vec());

        *unsafe { RECORDBUFF.get_mut() } = saved;
        *unsafe { LAST_RECORDED_LEN.get_mut() } = saved_len;
        unsafe { crate::globals::GLOBALS.get_mut() }.restart_edit = restart;
    }

    // --- merge_modifiers ---

    #[test]
    fn merge_modifiers_folds_ctrl_into_the_control_code() {
        // Cross-verified against real nvim: CTRL-a is 1.
        let mut m = i32::from(crate::keycodes_defs::MOD_MASK_CTRL);
        assert_eq!(merge_modifiers(i32::from(b'a'), &mut m), 1);
        assert_eq!(m, 0, "the CTRL bit is consumed once it is folded in");
    }

    #[test]
    fn merge_modifiers_turns_a_resulting_nul_into_k_zero() {
        // Cross-verified against real nvim: CTRL-@ becomes a special
        // key rather than a NUL byte, which cannot travel through the
        // input stream.
        let mut m = i32::from(crate::keycodes_defs::MOD_MASK_CTRL);
        assert_eq!(
            merge_modifiers(i32::from(b'@'), &mut m),
            crate::keycodes_defs::K_ZERO
        );
        assert_eq!(m, 0);
    }

    #[test]
    fn merge_modifiers_maps_ctrl_6_to_ctrl_caret() {
        // Cross-verified against real nvim: CTRL-^ is 30 (0x1e).
        let mut m = i32::from(crate::keycodes_defs::MOD_MASK_CTRL);
        assert_eq!(merge_modifiers(i32::from(b'6'), &mut m), 0x1e);
        assert_eq!(m, 0);
    }

    #[test]
    fn merge_modifiers_keeps_the_ctrl_bit_when_nothing_changed() {
        // A key with no control-code equivalent keeps its modifier so
        // a mapping can still match on it.
        let mut m = i32::from(crate::keycodes_defs::MOD_MASK_CTRL);
        let c = crate::keycodes_defs::K_LEFT;
        assert_eq!(merge_modifiers(c, &mut m), c);
        assert_eq!(m, i32::from(crate::keycodes_defs::MOD_MASK_CTRL));
    }

    #[test]
    fn merge_modifiers_without_ctrl_is_the_identity() {
        let mut m = i32::from(crate::keycodes_defs::MOD_MASK_SHIFT);
        assert_eq!(merge_modifiers(i32::from(b'a'), &mut m), i32::from(b'a'));
        assert_eq!(m, i32::from(crate::keycodes_defs::MOD_MASK_SHIFT));

        let mut none = 0;
        assert_eq!(merge_modifiers(i32::from(b'6'), &mut none), i32::from(b'6'));
    }

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
    fn save_typebuf_preserves_the_active_scripts_typeahead_and_resets_live_state() {
        let _lock = global_state_test_lock();
        reset_buffers();
        let saved = std::mem::replace(
            unsafe { SAVED_TYPEBUF.get_mut() },
            std::array::from_fn(|_| TypebufT::default()),
        );
        unsafe {
            *CURSCRIPT.get_mut() = 2;
            TYPEBUF.get_mut().tb_buf = vec![1, 2, 3];
            TYPEBUF.get_mut().tb_noremap = vec![4, 5, 6];
            TYPEBUF.get_mut().tb_len = 3;
            TYPEBUF.get_mut().tb_change_cnt = 9;
        }

        save_typebuf();

        assert_eq!(unsafe { SAVED_TYPEBUF.get_mut() }[2].tb_buf, vec![1, 2, 3]);
        assert_eq!(unsafe { SAVED_TYPEBUF.get_mut() }[2].tb_noremap, vec![4, 5, 6]);
        assert_eq!(unsafe { SAVED_TYPEBUF.get_mut() }[2].tb_len, 3);
        assert!(unsafe { TYPEBUF.get_mut() }.tb_buf.is_empty());
        assert!(unsafe { TYPEBUF.get_mut() }.tb_noremap.is_empty());
        assert_eq!(unsafe { TYPEBUF.get_mut() }.tb_len, 0);
        assert_eq!(unsafe { TYPEBUF.get_mut() }.tb_change_cnt, 10);

        *unsafe { SAVED_TYPEBUF.get_mut() } = saved;
        reset_buffers();
    }

    #[test]
    fn gotchars_state_defaults_with_room_for_escaped_multibyte_keys() {
        let state = GotcharsStateT::default();
        assert_eq!(
            state.buf.len(),
            crate::mbyte_defs::MB_MAXBYTES * 3 + 4
        );
        assert!(state.buf.iter().all(|&byte| byte == 0));
        assert_eq!(state.prev_c, 0);
        assert_eq!(state.buflen, 0);
        assert_eq!(state.pending_special, 0);
        assert_eq!(state.pending_mbyte, 0);
    }

    #[test]
    fn gotchars_add_byte_waits_for_complete_multibyte_and_special_keys() {
        let mut state = GotcharsStateT::default();
        assert!(gotchars_add_byte(&mut state, b'A'));
        assert_eq!(&state.buf[..state.buflen], b"A");

        state = GotcharsStateT::default();
        assert!(!gotchars_add_byte(&mut state, 0xc3));
        assert!(gotchars_add_byte(&mut state, 0xa9));
        assert_eq!(&state.buf[..state.buflen], "é".as_bytes());

        state = GotcharsStateT::default();
        assert!(!gotchars_add_byte(
            &mut state,
            crate::keycodes_defs::K_SPECIAL,
        ));
        assert!(!gotchars_add_byte(&mut state, b'k'));
        assert!(gotchars_add_byte(&mut state, b'u'));
        assert_eq!(state.prev_c, crate::keycodes_defs::K_UP);
    }

    #[test]
    fn gotchars_add_byte_waits_past_a_modifier_and_drops_completion_delay() {
        let mut state = GotcharsStateT::default();
        for byte in [
            crate::keycodes_defs::K_SPECIAL,
            crate::keycodes_defs::KS_MODIFIER,
            crate::keycodes_defs::MOD_MASK_SHIFT as u8,
        ] {
            assert!(!gotchars_add_byte(&mut state, byte));
        }
        assert!(gotchars_add_byte(&mut state, b'A'));
        assert_eq!(state.buflen, 4);

        state = GotcharsStateT::default();
        assert!(!gotchars_add_byte(
            &mut state,
            crate::keycodes_defs::K_SPECIAL,
        ));
        assert!(!gotchars_add_byte(
            &mut state,
            crate::keycodes_defs::key2termcap0(
                crate::keycodes_defs::K_COMPLETE_DELAY,
            ),
        ));
        assert!(gotchars_add_byte(
            &mut state,
            crate::keycodes_defs::key2termcap1(
                crate::keycodes_defs::K_COMPLETE_DELAY,
            ),
        ));
        assert_eq!(state.buflen, 0);
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
        assert_eq!(typebuf_len(), 0);
    }

    #[test]
    fn typebuf_len_reflects_tb_len() {
        let _lock = global_state_test_lock();
        reset_buffers();
        unsafe { TYPEBUF.get_mut() }.tb_len = 5;
        assert_eq!(typebuf_len(), 5);
        reset_buffers();
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

    /// Saves and restores the redo-buffer statics, so a failing test
    /// cannot leak them into another.
    struct RedobuffGuard {
        redo: BuffheaderT,
        old: BuffheaderT,
        read1: BuffheaderT,
        read2: BuffheaderT,
        reader: Option<RedoReaderState>,
        block: bool,
    }

    impl RedobuffGuard {
        fn new() -> Self {
            unsafe {
                Self {
                    redo: std::mem::take(REDOBUFF.get_mut()),
                    old: std::mem::take(OLD_REDOBUFF.get_mut()),
                    read1: std::mem::take(READBUF1.get_mut()),
                    read2: std::mem::take(READBUF2.get_mut()),
                    reader: REDO_READER.get_mut().take(),
                    block: *BLOCK_REDO.get_mut(),
                }
            }
        }
    }

    impl Drop for RedobuffGuard {
        fn drop(&mut self) {
            unsafe {
                *REDOBUFF.get_mut() = std::mem::take(&mut self.redo);
                *OLD_REDOBUFF.get_mut() = std::mem::take(&mut self.old);
                *READBUF1.get_mut() = std::mem::take(&mut self.read1);
                *READBUF2.get_mut() = std::mem::take(&mut self.read2);
                *REDO_READER.get_mut() = self.reader.take();
                *BLOCK_REDO.get_mut() = self.block;
            }
        }
    }

    /// The concatenated bytes currently held in a buffer.
    fn buff_bytes(buf: &BuffheaderT) -> Vec<u8> {
        buf.blocks.iter().flat_map(|b| b.b_str.clone()).collect()
    }

    /// Sets up a buffer/window and the `may_sync_undo` inputs, then
    /// reports whether `u_sync` actually ran.
    ///
    /// `b_p_ul` and the global `p_ul` are both forced negative so
    /// `u_sync` takes its simple "no entries, nothing to do" branch,
    /// which just sets `b_u_synced` - a clean observable effect.
    fn ran_u_sync(state: u32, arrow_used: bool, curscript: i32) -> bool {
        let mut buf = crate::buffer_defs::BufT {
            b_p_ul: -1,
            b_u_synced: false,
            ..Default::default()
        };
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_buf = globals.curbuf;
        let prev_state = globals.State;
        let prev_arrow = globals.Ins.arrow_used;
        let prev_script = unsafe { *CURSCRIPT.get_mut() };
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev_ul = opts.p_ul;
        opts.p_ul = -1;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.curbuf = &mut buf as *mut crate::buffer_defs::BufT;
        globals.State = state as i32;
        globals.Ins.arrow_used = arrow_used;
        unsafe { *CURSCRIPT.get_mut() = curscript };

        unsafe { may_sync_undo() };
        let ran = buf.b_u_synced;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.curbuf = prev_buf;
        globals.State = prev_state;
        globals.Ins.arrow_used = prev_arrow;
        unsafe { *CURSCRIPT.get_mut() = prev_script };
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ul = prev_ul;
        ran
    }

    #[test]
    fn may_sync_undo_syncs_outside_insert_and_cmdline() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(ran_u_sync(0, false, -1));
    }

    #[test]
    fn may_sync_undo_skips_mid_insert_until_a_cursor_key_is_used() {
        let _lock = crate::globals::global_state_test_lock();
        use crate::state_defs::mode::{CMDLINE, INSERT};

        // Mid-edit: syncing here would split the undo block.
        assert!(!ran_u_sync(INSERT, false, -1));
        assert!(!ran_u_sync(CMDLINE, false, -1));

        // A cursor key already ended the block, so syncing is fine.
        assert!(ran_u_sync(INSERT, true, -1));
        assert!(ran_u_sync(CMDLINE, true, -1));
    }

    #[test]
    fn may_sync_undo_never_syncs_while_reading_a_script() {
        let _lock = crate::globals::global_state_test_lock();
        // curscript >= 0 means a script is being read; that blocks the
        // sync even in the cases that would otherwise allow it.
        assert!(!ran_u_sync(0, false, 0));
        assert!(!ran_u_sync(crate::state_defs::mode::INSERT, true, 0));
    }

    #[test]
    fn stop_redo_ins_unblocks_the_redo_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe { *BLOCK_REDO.get_mut() = true };

        unsafe { stop_redo_ins() };
        assert!(!unsafe { *BLOCK_REDO.get_mut() });
        // Appends work again afterwards.
        unsafe { append_to_redobuff(b"ok") };
        assert_eq!(buff_bytes(unsafe { REDOBUFF.get_mut() }), b"ok".to_vec());
    }

    /// Saves and restores the `vungetc()` put-back state.
    struct OldCharGuard {
        chr: i32,
        mod_mask: i32,
        mouse_grid: i32,
        mouse_row: i32,
        mouse_col: i32,
        stuffed: bool,
        rb1: BuffheaderT,
        rb2: BuffheaderT,
    }

    impl OldCharGuard {
        fn new() -> Self {
            unsafe {
                Self {
                    chr: *OLD_CHAR.get_mut(),
                    mod_mask: *OLD_MOD_MASK.get_mut(),
                    mouse_grid: *OLD_MOUSE_GRID.get_mut(),
                    mouse_row: *OLD_MOUSE_ROW.get_mut(),
                    mouse_col: *OLD_MOUSE_COL.get_mut(),
                    stuffed: *OLD_KEY_STUFFED.get_mut(),
                    rb1: std::mem::take(READBUF1.get_mut()),
                    rb2: std::mem::take(READBUF2.get_mut()),
                }
            }
        }
    }

    impl Drop for OldCharGuard {
        fn drop(&mut self) {
            unsafe {
                *OLD_CHAR.get_mut() = self.chr;
                *OLD_MOD_MASK.get_mut() = self.mod_mask;
                *OLD_MOUSE_GRID.get_mut() = self.mouse_grid;
                *OLD_MOUSE_ROW.get_mut() = self.mouse_row;
                *OLD_MOUSE_COL.get_mut() = self.mouse_col;
                *OLD_KEY_STUFFED.get_mut() = self.stuffed;
                *READBUF1.get_mut() = std::mem::take(&mut self.rb1);
                *READBUF2.get_mut() = std::mem::take(&mut self.rb2);
            }
        }
    }

    #[test]
    fn vungetc_saves_the_character_and_all_current_input_metadata() {
        let _lock = crate::globals::global_state_test_lock();
        let _old = OldCharGuard::new();
        let _mod = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.mod_mask, 3)
        };
        let _grid = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.mouse_grid, 4)
        };
        let _row = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.mouse_row, 5)
        };
        let _col = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.mouse_col, 6)
        };
        let _stuffed = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.KeyStuffed, 1)
        };

        unsafe { vungetc(65) };

        assert_eq!(unsafe { *OLD_CHAR.get_mut() }, 65);
        assert_eq!(unsafe { *OLD_MOD_MASK.get_mut() }, 3);
        assert_eq!(unsafe { *OLD_MOUSE_GRID.get_mut() }, 4);
        assert_eq!(unsafe { *OLD_MOUSE_ROW.get_mut() }, 5);
        assert_eq!(unsafe { *OLD_MOUSE_COL.get_mut() }, 6);
        assert!(unsafe { *OLD_KEY_STUFFED.get_mut() });
    }

    #[test]
    fn vungetc_overwrites_the_previous_put_back_character() {
        let _lock = crate::globals::global_state_test_lock();
        let _old = OldCharGuard::new();
        let _stuffed = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.KeyStuffed, 0)
        };

        unsafe {
            vungetc(1);
            vungetc(2);
        }

        assert_eq!(unsafe { *OLD_CHAR.get_mut() }, 2);
        assert!(!unsafe { *OLD_KEY_STUFFED.get_mut() });
    }

    struct TypebufStateGuard(crate::input_defs::TypebufT);

    impl TypebufStateGuard {
        fn install(maplen: i32) -> Self {
            let replacement = crate::input_defs::TypebufT {
                tb_maplen: maplen,
                ..Default::default()
            };
            Self(unsafe { std::mem::replace(TYPEBUF.get_mut(), replacement) })
        }
    }

    impl Drop for TypebufStateGuard {
        fn drop(&mut self) {
            *unsafe { TYPEBUF.get_mut() } = std::mem::take(&mut self.0);
        }
    }

    #[test]
    fn check_end_reg_executing_defers_clearing_while_peeking() {
        let _lock = crate::globals::global_state_test_lock();
        let _typebuf = TypebufStateGuard::install(0);
        let _reg = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.reg_executing,
                i32::from(b'a'),
            )
        };
        let _pending = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.pending_end_reg_executing,
                false,
            )
        };

        unsafe { check_end_reg_executing(false) };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        assert_eq!(g.reg_executing, i32::from(b'a'));
        assert!(g.pending_end_reg_executing);
    }

    #[test]
    fn check_end_reg_executing_clears_once_input_advances() {
        let _lock = crate::globals::global_state_test_lock();
        let _typebuf = TypebufStateGuard::install(4);
        let _reg = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.reg_executing,
                i32::from(b'a'),
            )
        };
        let _pending = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.pending_end_reg_executing,
                true,
            )
        };

        unsafe { check_end_reg_executing(true) };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        assert_eq!(g.reg_executing, 0);
        assert!(!g.pending_end_reg_executing);
    }

    #[test]
    fn check_end_reg_executing_waits_for_mapped_typeahead() {
        let _lock = crate::globals::global_state_test_lock();
        let _typebuf = TypebufStateGuard::install(4);
        let _reg = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.reg_executing,
                i32::from(b'a'),
            )
        };
        let _pending = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.pending_end_reg_executing,
                false,
            )
        };

        unsafe { check_end_reg_executing(true) };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        assert_eq!(g.reg_executing, i32::from(b'a'));
        assert!(!g.pending_end_reg_executing);
    }

    #[test]
    fn can_get_old_char_needs_a_character_put_back() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = OldCharGuard::new();

        // -1 means nothing was put back.
        unsafe { *OLD_CHAR.get_mut() = -1 };
        assert!(!unsafe { can_get_old_char() });

        unsafe { *OLD_CHAR.get_mut() = i32::from(b'x') };
        unsafe { *OLD_KEY_STUFFED.get_mut() = false };
        assert!(unsafe { can_get_old_char() });
    }

    #[test]
    fn can_get_old_char_yields_to_pending_stuffed_characters() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = OldCharGuard::new();
        unsafe { *OLD_CHAR.get_mut() = i32::from(b'x') };

        // A character that was NOT stuffed must wait behind anything
        // since added to the stuff buffer.
        unsafe { *OLD_KEY_STUFFED.get_mut() = false };
        add_buff(unsafe { READBUF1.get_mut() }, b"pending");
        assert!(!unsafe { can_get_old_char() });

        // A character that WAS stuffed is taken regardless.
        unsafe { *OLD_KEY_STUFFED.get_mut() = true };
        assert!(unsafe { can_get_old_char() });
    }

    #[test]
    fn typeahead_noflush_stores_the_character() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { *TYPEAHEAD_CHAR.get_mut() };
        unsafe { typeahead_noflush(42) };
        assert_eq!(unsafe { *TYPEAHEAD_CHAR.get_mut() }, 42);
        unsafe { *TYPEAHEAD_CHAR.get_mut() = prev };
    }

    #[test]
    fn append_to_redobuff_accumulates() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe { *BLOCK_REDO.get_mut() = false };

        unsafe { append_to_redobuff(b"abc") };
        unsafe { append_to_redobuff(b"de") };
        assert_eq!(buff_bytes(unsafe { REDOBUFF.get_mut() }), b"abcde".to_vec());
    }

    #[test]
    fn append_number_and_char_to_redobuff() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe { *BLOCK_REDO.get_mut() = false };

        unsafe { append_number_to_redobuff(42) };
        unsafe { append_char_to_redobuff(i32::from(b'z')) };
        assert_eq!(buff_bytes(unsafe { REDOBUFF.get_mut() }), b"42z".to_vec());
    }

    #[test]
    fn read_redo_decodes_multibyte_special_and_literal_special_values() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe { *BLOCK_REDO.get_mut() = false };
        unsafe {
            append_char_to_redobuff(0x00e9);
            append_char_to_redobuff(crate::keycodes_defs::K_UP);
            append_char_to_redobuff(i32::from(crate::keycodes_defs::K_SPECIAL));
        }

        assert_eq!(unsafe { read_redo(true, false) }, crate::vim_defs::OK);
        assert_eq!(unsafe { read_redo(false, false) }, 0x00e9);
        assert_eq!(
            unsafe { read_redo(false, false) },
            crate::keycodes_defs::K_UP
        );
        assert_eq!(
            unsafe { read_redo(false, false) },
            i32::from(crate::keycodes_defs::K_SPECIAL)
        );
        assert_eq!(
            unsafe { read_redo(false, false) },
            i32::from(crate::ascii_defs::NUL)
        );
    }

    #[test]
    fn read_redo_selects_old_buffer_and_reports_empty_initialization() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe { *BLOCK_REDO.get_mut() = false };
        assert_eq!(unsafe { read_redo(true, false) }, crate::vim_defs::FAIL);

        unsafe { append_to_redobuff(b"old") };
        unsafe { reset_redobuff() };
        unsafe { append_to_redobuff(b"new") };

        assert_eq!(unsafe { read_redo(true, true) }, crate::vim_defs::OK);
        assert_eq!(unsafe { read_redo(false, true) }, i32::from(b'o'));
        assert_eq!(unsafe { read_redo(true, false) }, crate::vim_defs::OK);
        assert_eq!(unsafe { read_redo(false, false) }, i32::from(b'n'));
    }

    #[test]
    fn copy_redo_transfers_the_remaining_decoded_characters() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe { *BLOCK_REDO.get_mut() = false };
        unsafe {
            append_char_to_redobuff(i32::from(b'a'));
            append_char_to_redobuff(0x00e9);
            append_char_to_redobuff(crate::keycodes_defs::K_UP);
        }

        assert_eq!(unsafe { read_redo(true, false) }, crate::vim_defs::OK);
        assert_eq!(unsafe { read_redo(false, false) }, i32::from(b'a'));
        unsafe { copy_redo(false) };

        let mut expected = "é".as_bytes().to_vec();
        expected.extend_from_slice(&[
            crate::keycodes_defs::K_SPECIAL,
            crate::keycodes_defs::key2termcap0(crate::keycodes_defs::K_UP),
            crate::keycodes_defs::key2termcap1(crate::keycodes_defs::K_UP),
        ]);
        assert_eq!(buff_bytes(unsafe { READBUF2.get_mut() }), expected);
    }

    #[test]
    fn start_redo_replaces_the_old_count_and_increments_numbered_registers() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe { *BLOCK_REDO.get_mut() = false };
        unsafe { append_to_redobuff(b"\"22dw") };

        assert_eq!(unsafe { start_redo(5, false) }, crate::vim_defs::OK);
        assert_eq!(
            buff_bytes(unsafe { READBUF2.get_mut() }),
            b"\"35dw".to_vec()
        );
    }

    #[test]
    fn start_redo_returns_fail_for_an_empty_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        assert_eq!(unsafe { start_redo(0, false) }, crate::vim_defs::FAIL);
    }

    #[test]
    fn start_redo_ins_skips_count_and_command_then_blocks_recording() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe { *BLOCK_REDO.get_mut() = false };
        unsafe { append_to_redobuff(b"12ihello") };

        assert_eq!(unsafe { start_redo_ins() }, crate::vim_defs::OK);
        assert_eq!(
            buff_bytes(unsafe { READBUF2.get_mut() }),
            b"hello".to_vec()
        );
        assert!(unsafe { *BLOCK_REDO.get_mut() });
    }

    #[test]
    fn start_redo_ins_adds_a_newline_for_open_line_commands() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe { *BLOCK_REDO.get_mut() = false };
        unsafe { append_to_redobuff(b"2otext") };

        assert_eq!(unsafe { start_redo_ins() }, crate::vim_defs::OK);
        assert_eq!(
            buff_bytes(unsafe { READBUF2.get_mut() }),
            b"\ntext".to_vec()
        );
    }

    #[test]
    fn prep_redo_num2_serializes_register_counts_and_commands() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe { *BLOCK_REDO.get_mut() = false };

        unsafe {
            crate::normal::prep_redo_num2(
                i32::from(b'a'),
                12,
                i32::from(b'g'),
                i32::from(b'q'),
                3,
                i32::from(b'x'),
                0,
                i32::from(b'z'),
            )
        };

        assert_eq!(
            buff_bytes(unsafe { REDOBUFF.get_mut() }),
            b"\"a12gq3xz".to_vec()
        );
    }

    #[test]
    fn prep_redo_omits_the_second_count_slot() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe { *BLOCK_REDO.get_mut() = false };

        unsafe {
            crate::normal::prep_redo(
                0,
                4,
                i32::from(b'd'),
                i32::from(b'w'),
                0,
                0,
                0,
            )
        };

        assert_eq!(buff_bytes(unsafe { REDOBUFF.get_mut() }), b"4dw".to_vec());
    }

    #[test]
    fn get_inserted_returns_none_for_an_empty_redo_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe { REDOBUFF.get_mut() }.blocks.clear();

        assert_eq!(get_inserted(), None);
    }

    #[test]
    fn get_inserted_returns_all_redo_blocks_without_consuming_them() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe {
            add_buff(REDOBUFF.get_mut(), b"ab");
            add_buff(REDOBUFF.get_mut(), b"cd");
        }

        assert_eq!(get_inserted(), Some(b"abcd".to_vec()));
        assert_eq!(get_inserted(), Some(b"abcd".to_vec()));
        assert_eq!(unsafe { REDOBUFF.get_mut().blocks.len() }, 2);
    }

    #[test]
    fn block_redo_suppresses_every_append() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe { *BLOCK_REDO.get_mut() = true };

        unsafe { append_to_redobuff(b"abc") };
        unsafe { append_char_to_redobuff(i32::from(b'z')) };
        unsafe { append_number_to_redobuff(42) };
        assert!(unsafe { REDOBUFF.get_mut() }.blocks.is_empty());
    }

    #[test]
    fn reset_redobuff_moves_the_current_buffer_aside() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe { *BLOCK_REDO.get_mut() = false };
        unsafe { append_to_redobuff(b"current") };

        unsafe { reset_redobuff() };

        // The redo buffer starts over, and the previous contents are
        // kept for CTRL-O '.'.
        assert!(unsafe { REDOBUFF.get_mut() }.blocks.is_empty());
        assert_eq!(buff_bytes(unsafe { OLD_REDOBUFF.get_mut() }), b"current".to_vec());
    }

    #[test]
    fn reset_redobuff_does_nothing_while_redo_is_blocked() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe { *BLOCK_REDO.get_mut() = false };
        unsafe { append_to_redobuff(b"keep") };
        unsafe { *BLOCK_REDO.get_mut() = true };

        unsafe { reset_redobuff() };

        // The buffer is left intact rather than being rotated away.
        assert_eq!(buff_bytes(unsafe { REDOBUFF.get_mut() }), b"keep".to_vec());
        assert!(unsafe { OLD_REDOBUFF.get_mut() }.blocks.is_empty());
    }

    #[test]
    fn cancel_redo_restores_previous_redo_and_drains_stuff_buffers() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe { *BLOCK_REDO.get_mut() = false };
        unsafe { append_to_redobuff(b"previous") };
        unsafe { reset_redobuff() };
        unsafe { append_to_redobuff(b"discard") };
        stuff_readbuff(b"one");
        stuff_redo_readbuff(b"two");

        unsafe { cancel_redo() };

        assert_eq!(
            buff_bytes(unsafe { REDOBUFF.get_mut() }),
            b"previous".to_vec()
        );
        assert!(unsafe { OLD_REDOBUFF.get_mut() }.blocks.is_empty());
        assert!(stuff_empty());
    }

    #[test]
    fn cancel_redo_is_inert_while_redo_is_blocked() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe {
            *BLOCK_REDO.get_mut() = false;
            append_to_redobuff(b"keep");
            *BLOCK_REDO.get_mut() = true;
            cancel_redo();
        }
        assert_eq!(buff_bytes(unsafe { REDOBUFF.get_mut() }), b"keep".to_vec());
    }

    #[test]
    fn restore_redobuff_puts_both_buffers_back() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe { *BLOCK_REDO.get_mut() = false };
        // Something else is in the buffers at restore time; it must be
        // replaced, not appended to.
        unsafe { append_to_redobuff(b"discard me") };

        let mut saved = crate::input_defs::SaveRedoT::default();
        add_buff(&mut saved.sr_redobuff, b"saved");
        add_buff(&mut saved.sr_old_redobuff, b"saved-old");

        unsafe { restore_redobuff(&mut saved) };

        assert_eq!(buff_bytes(unsafe { REDOBUFF.get_mut() }), b"saved".to_vec());
        assert_eq!(buff_bytes(unsafe { OLD_REDOBUFF.get_mut() }), b"saved-old".to_vec());
    }

    #[test]
    fn save_redobuff_moves_both_buffers_and_keeps_a_nested_redo_copy() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        unsafe {
            *BLOCK_REDO.get_mut() = false;
            add_buff(REDOBUFF.get_mut(), b"ab");
            add_buff(REDOBUFF.get_mut(), b"cd");
            add_buff(OLD_REDOBUFF.get_mut(), b"older");
        }
        let mut saved = crate::input_defs::SaveRedoT::default();

        unsafe { save_redobuff(&mut saved) };

        assert_eq!(buff_bytes(&saved.sr_redobuff), b"abcd".to_vec());
        assert_eq!(buff_bytes(&saved.sr_old_redobuff), b"older".to_vec());
        assert_eq!(
            buff_bytes(unsafe { REDOBUFF.get_mut() }),
            b"abcd".to_vec()
        );
        assert!(unsafe { OLD_REDOBUFF.get_mut() }.blocks.is_empty());

        unsafe { restore_redobuff(&mut saved) };
        assert_eq!(
            buff_bytes(unsafe { REDOBUFF.get_mut() }),
            b"abcd".to_vec()
        );
        assert_eq!(
            buff_bytes(unsafe { OLD_REDOBUFF.get_mut() }),
            b"older".to_vec()
        );
    }

    #[test]
    fn restore_redobuff_ignores_block_redo() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = RedobuffGuard::new();
        // Unlike the appends and ResetRedobuff, the original's
        // restoreRedobuff has no block_redo guard at all.
        unsafe { *BLOCK_REDO.get_mut() = true };

        let mut saved = crate::input_defs::SaveRedoT::default();
        add_buff(&mut saved.sr_redobuff, b"saved");

        unsafe { restore_redobuff(&mut saved) };
        assert_eq!(buff_bytes(unsafe { REDOBUFF.get_mut() }), b"saved".to_vec());
    }

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
    fn stuff_readbuff_len_appends_the_bounded_slice() {
        let _lock = global_state_test_lock();
        reset_buffers();

        stuff_readbuff_len(&b"abcdef"[..3]);

        assert_eq!(
            buff_bytes(unsafe { READBUF1.get_mut() }),
            b"abc"
        );
        reset_buffers();
    }

    #[test]
    fn stuff_readbuff_spec_preserves_special_keys_and_replaces_controls() {
        let _lock = global_state_test_lock();
        reset_buffers();
        let input = [
            crate::keycodes_defs::K_SPECIAL,
            b'k',
            b'u',
            crate::ascii_defs::CAR,
            crate::ascii_defs::NL,
            crate::ascii_defs::ESC,
            b'x',
            crate::ascii_defs::NUL,
            b'y',
        ];

        stuff_readbuff_spec(&input);

        assert_eq!(
            buff_bytes(unsafe { READBUF1.get_mut() }),
            vec![
                crate::keycodes_defs::K_SPECIAL,
                b'k',
                b'u',
                b' ',
                b' ',
                b' ',
                b'x',
            ]
        );
        reset_buffers();
    }

    #[test]
    fn stuff_readbuff_spec_preserves_multibyte_text() {
        let _lock = global_state_test_lock();
        reset_buffers();

        stuff_readbuff_spec("é".as_bytes());

        assert_eq!(
            buff_bytes(unsafe { READBUF1.get_mut() }),
            "é".as_bytes()
        );
        reset_buffers();
    }

    fn current_typebuf() -> (Vec<u8>, Vec<u8>) {
        let tb = unsafe { TYPEBUF.get_mut() };
        let start = tb.tb_off as usize;
        let end = start + tb.tb_len as usize;
        (
            tb.tb_buf[start..end].to_vec(),
            tb.tb_noremap[start..end].to_vec(),
        )
    }

    #[test]
    fn ins_typebuf_inserts_at_the_front_and_updates_change_count() {
        let _lock = global_state_test_lock();
        let _typebuf = TypebufStateGuard::install(0);

        assert_eq!(
            ins_typebuf(
                b"abc",
                crate::input_defs::RemapValues::Yes as i32,
                0,
                false,
                false,
            ),
            crate::vim_defs::OK
        );

        assert_eq!(current_typebuf(), (b"abc".to_vec(), vec![0, 0, 0]));
        let tb = unsafe { TYPEBUF.get_mut() };
        assert_eq!(tb.tb_off, MAXMAPLEN + 4 - 3);
        assert_eq!(tb.tb_change_cnt, 2);
    }

    #[test]
    fn ins_typebuf_inserts_in_the_middle_and_tracks_mapped_silent_bytes() {
        let _lock = global_state_test_lock();
        let _typebuf = TypebufStateGuard::install(0);
        let _cmd_silent = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.cmd_silent,
                false,
            )
        };
        assert_eq!(
            ins_typebuf(
                b"abc",
                crate::input_defs::RemapValues::Yes as i32,
                0,
                false,
                false,
            ),
            crate::vim_defs::OK
        );

        assert_eq!(
            ins_typebuf(
                b"X",
                crate::input_defs::RemapValues::None as i32,
                1,
                true,
                true,
            ),
            crate::vim_defs::OK
        );

        assert_eq!(
            current_typebuf(),
            (b"aXbc".to_vec(), vec![RM_YES as u8, RM_NONE as u8, RM_YES as u8, RM_YES as u8])
        );
        let tb = unsafe { TYPEBUF.get_mut() };
        assert_eq!(tb.tb_maplen, 1);
        assert_eq!(tb.tb_silent, 1);
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.cmd_silent);
    }

    #[test]
    fn ins_typebuf_assigns_skip_script_and_counted_remap_flags() {
        let _lock = global_state_test_lock();
        for (noremap, expected) in [
            (
                crate::input_defs::RemapValues::Skip as i32,
                vec![RM_ABBR as u8, RM_YES as u8, RM_YES as u8],
            ),
            (
                crate::input_defs::RemapValues::Script as i32,
                vec![RM_SCRIPT as u8; 3],
            ),
            (2, vec![RM_NONE as u8, RM_NONE as u8, RM_YES as u8]),
        ] {
            let _typebuf = TypebufStateGuard::install(0);
            assert_eq!(
                ins_typebuf(b"abc", noremap, 0, false, false),
                crate::vim_defs::OK
            );
            assert_eq!(current_typebuf().1, expected);
        }
    }

    #[test]
    fn ins_typebuf_extends_the_no_abbreviation_prefix() {
        let _lock = global_state_test_lock();
        let _typebuf = TypebufStateGuard::install(0);
        init_typebuf();
        unsafe { TYPEBUF.get_mut() }.tb_no_abbr_cnt = 2;

        assert_eq!(
            ins_typebuf(
                b"xy",
                crate::input_defs::RemapValues::Yes as i32,
                0,
                false,
                false,
            ),
            crate::vim_defs::OK
        );

        assert_eq!(unsafe { TYPEBUF.get_mut() }.tb_no_abbr_cnt, 4);
    }

    #[test]
    fn ins_typebuf_wraps_change_count_away_from_zero_and_stops_at_nul() {
        let _lock = global_state_test_lock();
        let _typebuf = TypebufStateGuard::install(0);
        init_typebuf();
        unsafe { TYPEBUF.get_mut() }.tb_change_cnt = -1;

        assert_eq!(
            ins_typebuf(
                b"a\0ignored",
                crate::input_defs::RemapValues::Yes as i32,
                0,
                false,
                false,
            ),
            crate::vim_defs::OK
        );

        assert_eq!(current_typebuf().0, b"a");
        assert_eq!(unsafe { TYPEBUF.get_mut() }.tb_change_cnt, 1);
    }

    #[test]
    fn del_typebuf_removes_front_bytes_by_advancing_the_offset() {
        let _lock = global_state_test_lock();
        let _typebuf = TypebufStateGuard::install(0);
        assert_eq!(
            ins_typebuf(
                b"abcdef",
                crate::input_defs::RemapValues::Yes as i32,
                0,
                false,
                false,
            ),
            crate::vim_defs::OK
        );
        let old_off = unsafe { TYPEBUF.get_mut() }.tb_off;

        del_typebuf(2, 0);

        assert_eq!(current_typebuf().0, b"cdef");
        assert_eq!(unsafe { TYPEBUF.get_mut() }.tb_off, old_off + 2);
    }

    #[test]
    fn del_typebuf_removes_middle_bytes_and_parallel_remap_flags() {
        let _lock = global_state_test_lock();
        let _typebuf = TypebufStateGuard::install(0);
        assert_eq!(
            ins_typebuf(b"abcdef", 3, 0, false, false),
            crate::vim_defs::OK
        );

        del_typebuf(2, 2);

        assert_eq!(
            current_typebuf(),
            (b"abef".to_vec(), vec![RM_NONE as u8, RM_NONE as u8, RM_YES as u8, RM_YES as u8])
        );
    }

    #[test]
    fn del_typebuf_clamps_or_decrements_prefix_counters() {
        let _lock = global_state_test_lock();
        let _typebuf = TypebufStateGuard::install(0);
        assert_eq!(
            ins_typebuf(b"abcdef", 0, 0, false, false),
            crate::vim_defs::OK
        );
        {
            let tb = unsafe { TYPEBUF.get_mut() };
            tb.tb_maplen = 3;
            tb.tb_silent = 5;
            tb.tb_no_abbr_cnt = 6;
        }

        del_typebuf(2, 2);

        let tb = unsafe { TYPEBUF.get_mut() };
        assert_eq!(tb.tb_maplen, 2);
        assert_eq!(tb.tb_silent, 3);
        assert_eq!(tb.tb_no_abbr_cnt, 4);
    }

    #[test]
    fn del_typebuf_resets_filled_and_wraps_change_count_away_from_zero() {
        let _lock = global_state_test_lock();
        let _typebuf = TypebufStateGuard::install(0);
        let _filled = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.typebuf_was_filled,
                true,
            )
        };
        assert_eq!(
            ins_typebuf(
                b"x",
                crate::input_defs::RemapValues::Yes as i32,
                0,
                false,
                false,
            ),
            crate::vim_defs::OK
        );
        unsafe { TYPEBUF.get_mut() }.tb_change_cnt = -1;
        unsafe { crate::globals::GLOBALS.get_mut() }.typebuf_was_filled = true;

        del_typebuf(1, 0);

        assert!(current_typebuf().0.is_empty());
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.typebuf_was_filled);
        assert_eq!(unsafe { TYPEBUF.get_mut() }.tb_change_cnt, 1);
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
    fn stuffescaped_prefixes_literal_controls_and_preserves_printable_runs() {
        let _lock = global_state_test_lock();
        reset_buffers();

        stuffescaped(b"ab\x01\t\x7f", true);

        assert_eq!(
            buff_bytes(unsafe { READBUF1.get_mut() }),
            vec![
                b'a',
                b'b',
                crate::ascii_defs::CTRL_V,
                1,
                crate::ascii_defs::TAB,
                crate::ascii_defs::CTRL_V,
                crate::ascii_defs::DEL,
            ]
        );
        reset_buffers();
    }

    #[test]
    fn stuffescaped_keeps_interpreted_special_sequences_and_multibyte_text() {
        let _lock = global_state_test_lock();
        reset_buffers();
        let special = [crate::keycodes_defs::K_SPECIAL, b'k', b'u'];
        stuffescaped(&special, false);
        stuffescaped("é".as_bytes(), false);

        let mut expected = special.to_vec();
        expected.extend_from_slice("é".as_bytes());
        assert_eq!(buff_bytes(unsafe { READBUF1.get_mut() }), expected);
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

    // --- free_buff / alloc_typebuf / free_typebuf ---

    #[test]
    fn free_buff_clears_blocks_but_leaves_bh_index_untouched() {
        let mut buf = BuffheaderT {
            blocks: vec![BuffblockT { b_str: b"x".to_vec() }],
            bh_index: 7,
        };
        free_buff(&mut buf);
        assert!(buf.blocks.is_empty());
        assert_eq!(buf.bh_index, 7, "free_buff never touches bh_index, matching the original");
    }

    #[test]
    fn init_typebuf_initializes_the_static_buffer_once() {
        let _lock = global_state_test_lock();
        let _g = TypebufStateGuard::install(0);

        init_typebuf();

        let tb = unsafe { TYPEBUF.get_mut() };
        assert_eq!(tb.tb_off, MAXMAPLEN + 4);
        assert_eq!(tb.tb_len, 0);
        assert_eq!(tb.tb_change_cnt, 1);
    }

    #[test]
    fn init_typebuf_leaves_an_initialized_buffer_untouched() {
        let _lock = global_state_test_lock();
        let _g = TypebufStateGuard::install(0);
        unsafe {
            TYPEBUF.get_mut().tb_buf = vec![1, 2, 3];
            TYPEBUF.get_mut().tb_off = 9;
            TYPEBUF.get_mut().tb_change_cnt = 7;
        }

        init_typebuf();

        let tb = unsafe { TYPEBUF.get_mut() };
        assert_eq!(tb.tb_buf, vec![1, 2, 3]);
        assert_eq!(tb.tb_off, 9);
        assert_eq!(tb.tb_change_cnt, 7);
    }

    #[test]
    fn alloc_typebuf_resets_fields_and_increments_change_cnt() {
        let _lock = global_state_test_lock();
        reset_buffers();
        unsafe { TYPEBUF.get_mut() }.tb_buf = vec![1, 2, 3];
        unsafe { TYPEBUF.get_mut() }.tb_len = 5;
        unsafe { TYPEBUF.get_mut() }.tb_change_cnt = 10;
        unsafe { crate::globals::GLOBALS.get_mut() }.typebuf_was_filled = true;

        alloc_typebuf();

        let tb = unsafe { TYPEBUF.get_mut() };
        assert!(tb.tb_buf.is_empty());
        assert!(tb.tb_noremap.is_empty());
        assert_eq!(tb.tb_off, MAXMAPLEN + 4);
        assert_eq!(tb.tb_len, 0);
        assert_eq!(tb.tb_change_cnt, 11);
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.typebuf_was_filled);
        reset_buffers();
    }

    #[test]
    fn alloc_typebuf_wraps_change_cnt_from_negative_one_to_one() {
        let _lock = global_state_test_lock();
        reset_buffers();
        unsafe { TYPEBUF.get_mut() }.tb_change_cnt = -1;
        alloc_typebuf();
        assert_eq!(unsafe { TYPEBUF.get_mut() }.tb_change_cnt, 1);
        reset_buffers();
    }

    // --- save_typeahead / restore_typeahead ---

    fn reset_old_char() {
        unsafe {
            *OLD_CHAR.get_mut() = -1;
            *OLD_MOD_MASK.get_mut() = 0;
        }
    }

    #[test]
    fn save_typeahead_captures_and_resets_all_three_buffers() {
        let _lock = global_state_test_lock();
        reset_buffers();
        reset_old_char();

        unsafe { TYPEBUF.get_mut() }.tb_buf = vec![1, 2, 3];
        unsafe { TYPEBUF.get_mut() }.tb_len = 5;
        unsafe { TYPEBUF.get_mut() }.tb_change_cnt = 10;
        unsafe { READBUF1.get_mut() }.blocks.push(BuffblockT { b_str: b"abc".to_vec() });
        unsafe { READBUF2.get_mut() }.blocks.push(BuffblockT { b_str: b"xyz".to_vec() });
        unsafe {
            *OLD_CHAR.get_mut() = 42;
            *OLD_MOD_MASK.get_mut() = 7;
        }

        let mut tp = crate::input_defs::TasaveT::default();
        save_typeahead(&mut tp);

        // Captured into tp:
        assert!(tp.typebuf_valid);
        assert_eq!(tp.save_typebuf.tb_buf, vec![1, 2, 3]);
        assert_eq!(tp.save_typebuf.tb_len, 5);
        assert_eq!(tp.save_typebuf.tb_change_cnt, 10);
        assert_eq!(tp.old_char, 42);
        assert_eq!(tp.old_mod_mask, 7);
        assert_eq!(tp.save_readbuf1.blocks[0].b_str, b"abc");
        assert_eq!(tp.save_readbuf2.blocks[0].b_str, b"xyz");

        // Live state reset:
        let tb = unsafe { TYPEBUF.get_mut() };
        assert!(tb.tb_buf.is_empty());
        assert_eq!(tb.tb_len, 0);
        assert_eq!(tb.tb_change_cnt, 11);
        assert_eq!(unsafe { *OLD_CHAR.get_mut() }, -1);
        assert!(unsafe { READBUF1.get_mut() }.blocks.is_empty());
        assert!(unsafe { READBUF2.get_mut() }.blocks.is_empty());

        reset_buffers();
        reset_old_char();
    }

    #[test]
    fn restore_typeahead_round_trips_through_save_typeahead() {
        let _lock = global_state_test_lock();
        reset_buffers();
        reset_old_char();

        unsafe { TYPEBUF.get_mut() }.tb_buf = vec![9, 9, 9];
        unsafe { TYPEBUF.get_mut() }.tb_len = 3;
        unsafe { READBUF1.get_mut() }.blocks.push(BuffblockT { b_str: b"one".to_vec() });
        unsafe { READBUF2.get_mut() }.blocks.push(BuffblockT { b_str: b"two".to_vec() });
        unsafe {
            *OLD_CHAR.get_mut() = 5;
            *OLD_MOD_MASK.get_mut() = 1;
        }

        let mut tp = crate::input_defs::TasaveT::default();
        save_typeahead(&mut tp);
        restore_typeahead(&mut tp);

        assert_eq!(unsafe { TYPEBUF.get_mut() }.tb_buf, vec![9, 9, 9]);
        assert_eq!(unsafe { TYPEBUF.get_mut() }.tb_len, 3);
        assert_eq!(unsafe { *OLD_CHAR.get_mut() }, 5);
        assert_eq!(unsafe { *OLD_MOD_MASK.get_mut() }, 1);
        assert_eq!(unsafe { READBUF1.get_mut() }.blocks[0].b_str, b"one");
        assert_eq!(unsafe { READBUF2.get_mut() }.blocks[0].b_str, b"two");

        reset_buffers();
        reset_old_char();
    }

    #[test]
    fn restore_typeahead_skips_typebuf_when_not_valid_but_still_restores_the_rest() {
        let _lock = global_state_test_lock();
        reset_buffers();
        reset_old_char();

        unsafe { TYPEBUF.get_mut() }.tb_len = 77;

        let mut tp = crate::input_defs::TasaveT {
            typebuf_valid: false,
            save_typebuf: crate::input_defs::TypebufT { tb_len: 999, ..Default::default() },
            old_char: 5,
            old_mod_mask: 1,
            save_readbuf1: BuffheaderT { blocks: vec![BuffblockT { b_str: b"r1".to_vec() }], bh_index: 0 },
            save_readbuf2: BuffheaderT::default(),
            save_inputbuf: Default::default(),
        };
        restore_typeahead(&mut tp);

        // typebuf itself is untouched since typebuf_valid was false:
        assert_eq!(unsafe { TYPEBUF.get_mut() }.tb_len, 77);
        // But old_char/old_mod_mask/readbufs are restored unconditionally:
        assert_eq!(unsafe { *OLD_CHAR.get_mut() }, 5);
        assert_eq!(unsafe { *OLD_MOD_MASK.get_mut() }, 1);
        assert_eq!(unsafe { READBUF1.get_mut() }.blocks[0].b_str, b"r1");

        reset_buffers();
        reset_old_char();
    }
}
