//! Translated from `src/nvim/change.c` (partial).
//!
//! `change.c` (~2200 lines) is the buffer-modification/change-tracking
//! core (`changed`/`changed_bytes`/`changed_lines`, insert-mode byte
//! insertion, indent-preservation helpers, etc.). `changed_common`/
//! `changed_lines_invalidate_win`/`changed_lines`/`changed_bytes` (etc.)
//! still need a wide spread of OTHER not-yet-translated subsystems:
//! window/fold display bookkeeping (`find_wl_entry`,
//! `invalidate_botline_win`, `buf_meta_total`), and more.
//!
//! Translated here: `save_file_ff` (snapshots `'fileformat'`/
//! `'fileencoding'`/`'endofline'`/`'bomb'` so a later `file_ff_differs`
//! call can detect a change - always assigns a fresh `b_start_fenc`
//! clone rather than replicating the original's own "only alloc when
//! the value differs" `xfree`/`strcmp`/`xstrdup` micro-optimization,
//! since Rust's `Option<Vec<u8>>` ownership makes the two observably
//! identical), `file_ff_differs` (needed by `undo.c`'s
//! `bufIsChanged`) and `change_warning` (needed a real `apply_autocmds`
//! plus `autocmd_busy`, both now available). `change_warning`'s own
//! real message display (`msg_start`/`msg_source`/`msg_puts_hl`/
//! `msg_clr_eos`/`msg_end`/`msg_delay`/`showmode`) is skipped -
//! `message.c`'s display pipeline is not yet tractable - but every
//! OTHER observable state change is kept faithfully, including
//! `set_vim_var_string(VV_WARNINGMSG, ...)`: `set_vim_var_string`
//! itself only writes directly to the `VIMVARS` storage slot
//! (`crate::eval::vars::VIMVARS[idx].di.di_tv`), which is real and
//! requires no dict/hashtable wiring at all - confirmed by reading
//! its own body before wiring this call in for real. (`evalvars_init`,
//! the full `v:` scope-dict bootstrap, is now ALSO translated, but
//! `change_warning` never needed to wait for it - this call worked
//! correctly even before `evalvars_init` existed.)
//!
//! Also translated: `get_leader_len` (scans a comment leader at the
//! START of `line`, per `'comments'`) - a genuinely intricate
//! algorithm (nested/middle/end comment markers, backward-vs-forward
//! `'O'`-flag gating, a growing-buffer-free byte-offset translation of
//! the original's own reused `part_buf[COM_MAX_LEN]` stack array),
//! needing only already-real `option::copy_option_part`/
//! `strings::vim_strchr`/`ascii_defs::ascii_iswhite` plus
//! `BufT.b_p_com`. `flags: Option<&mut usize>` models the original's
//! nullable `char **flags` out-parameter as a byte offset into
//! `b_p_com` (not a raw pointer), matching this crate's own
//! byte-offset-instead-of-pointer idiom used throughout (`path.rs`,
//! `mbyte.rs`, etc.). Preserves one real, obscure original quirk
//! literally rather than "fixing" it (translate faithfully, bugs and
//! all): `middle_match_len` doubles as both a length AND a
//! "no middle match yet" sentinel (`0` for both), so a middle-comment
//! definition whose own string half is genuinely empty is
//! indistinguishable from "no middle match found" - an extremely
//! obscure edge case for any real `'comments'` value, not something
//! any real session's own default value can trigger.
//!
//! Also translated: `get_last_leader_offset` (the backward-scanning
//! sibling - finds the offset of the LAST comment leader in `line`,
//! scanning right-to-left, with its own substring-verification pass
//! adjusting how far back the scan needs to go for a nested comment),
//! needing the exact same dependencies as `get_leader_len` and no new
//! infrastructure. This directly unblocks `ops.c`'s own `skip_comment`
//! (see `ops.rs`).
//!
//! Also translated: [`changed`] itself - re-examined and found
//! tractable now that `changed_internal`/`buf_inc_changedtick` are
//! both real: its own real "create a swap file" branch is gated behind
//! `BufT.b_may_swap`, which `ml_open` only ever sets `true` when
//! `OPTION_VARS.p_uc != 0` - always `false` today, since nothing
//! bootstraps real option defaults for `'updatecount'` yet (matching
//! this crate's own established "`OPTION_VARS` defaults to raw C
//! zero-init, not the real post-startup value" convention) - the real,
//! always-false-today check is kept (not hardcoded away), with its own
//! body `unimplemented!()`ing if ever genuinely reached (needing
//! `ml_open_file`, real swap-file creation, plus the message-display
//! pipeline).
//!
//! Also translated: [`changed_lines_redraw_buf`] - maintains the
//! `b_mod_*` "region that must be redisplayed" bookkeeping, widening
//! it across repeated changes - and
//! [`changed_lines_invalidate_win`]/[`changed_lines_invalidate_buf`],
//! which invalidate the cached cursor/botline values and renumber the
//! `w_lines[]` display cache after a change. All three are fully
//! faithful: they touch only already-real `BufT`/`WinT` fields plus
//! `buf_meta_total`, `fold.rs`'s `find_wl_entry` and `move.rs`'s own
//! cache-invalidation helpers, so they need none of the display
//! pipeline (which only later reads what they record).
//!
//! Deferred: everything else in the file - each is its own substantial
//! undertaking blocked on subsystems not yet translated (the display
//! pipeline, the fold/diff subsystems, etc. - see above).

use crate::ascii_defs::ascii_iswhite;
use crate::buffer_defs::{b_flags, BufT, WinT};
use crate::option::copy_option_part;
use crate::pos_defs::{ColnrT, LinenrT};
use crate::strings::vim_strchr;

/// Invalidate the cached cursor/botline values and the `w_lines[]`
/// display cache of window `wp` after lines `lnum..lnume` changed
/// (`changed_lines_invalidate_win`).
///
/// `xtra` is the number of lines added (positive) or removed
/// (negative). Entries below the change have their line numbers
/// corrected so display can stop early; entries covered by the change
/// are invalidated.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose
/// `w_buffer` is valid and whose marktree is well-formed.
pub unsafe fn changed_lines_invalidate_win(
    wp: *mut WinT,
    lnum: LinenrT,
    col: ColnrT,
    lnume: LinenrT,
    xtra: LinenrT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    let mut lnume = lnume;

    // If the changed line is in a range of previously folded lines,
    // compare with the first line in that range.
    if w.w_cursor.lnum <= lnum {
        // (`find_wl_entry` returns `Option<usize>` here, the checked
        // form of the original's `int i` / `i >= 0` sentinel.)
        let found = crate::fold::find_wl_entry(w, lnum);
        if let Some(i) = found
            && w.w_cursor.lnum > w.w_lines[i].wl_lnum
        {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::r#move::changed_line_abv_curs_win(w) };
        }
    }

    if w.w_cursor.lnum > lnum {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::r#move::changed_line_abv_curs_win(w) };
    } else if w.w_cursor.lnum == lnum && w.w_cursor.col >= col {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::r#move::changed_cline_bef_curs(w) };
    }
    if w.w_botline >= lnum {
        if xtra < 0 {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::r#move::invalidate_botline_win(w) };
        } else {
            // Assume that botline doesn't change (inserted lines make
            // other lines scroll down below botline).
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::r#move::approximate_botline_win(w) };
        }
    }

    // If lines have been inserted/deleted and the buffer has
    // virt_lines, or inline virt_text with 'wrap' enabled, invalidate
    // the line after the changed lines: virt_lines may now be drawn
    // above that line, and inline virt_text may cause it to wrap.
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &*w.w_buffer };
    if (xtra < 0
        && w.w_onebuf_opt.wo_wrap != 0
        && crate::buffer::buf_meta_total(buf, crate::marktree_defs::MetaIndex::Inline) != 0)
        || (xtra != 0
            && crate::buffer::buf_meta_total(buf, crate::marktree_defs::MetaIndex::Lines) != 0)
    {
        lnume += 1;
    }

    // Check if any w_lines[] entries have become invalid. For entries
    // below the change, correct the lnums for inserted/deleted lines,
    // which makes it possible to stop displaying after the change.
    for i in 0..w.w_lines_valid {
        let entry = &mut w.w_lines[i as usize];
        if !entry.wl_valid {
            continue;
        }
        if entry.wl_lnum >= lnum {
            // Do not change wl_lnum at index zero, it is used to
            // compare with w_topline. Invalidate it instead.
            if i == 0 || entry.wl_lnum < lnume {
                // Line included in the change.
                entry.wl_valid = false;
            } else if xtra != 0 {
                // Line below the change.
                entry.wl_lnum += xtra;
                entry.wl_foldend += xtra;
                entry.wl_lastlnum += xtra;
            }
        } else if entry.wl_lastlnum >= lnum {
            // Change somewhere inside this range of folded or
            // concealed lines, so it may need to be redrawn.
            entry.wl_valid = false;
        }
    }
}

/// Like [`changed_lines_invalidate_win`], but for every window
/// displaying `buf` (`changed_lines_invalidate_buf`).
///
/// # Safety
/// Same as [`changed_lines_invalidate_win`], for every window in
/// `GLOBALS.firstwin`'s own `w_next` chain.
pub unsafe fn changed_lines_invalidate_buf(
    buf: *mut BufT,
    lnum: LinenrT,
    col: ColnrT,
    lnume: LinenrT,
    xtra: LinenrT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let w = unsafe { &*wp };
        let next = w.w_next;
        if w.w_buffer == buf {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { changed_lines_invalidate_win(wp, lnum, col, lnume, xtra) };
        }
        wp = next;
    }
}

/// Delete `nlines` lines at the cursor (`del_lines`).
///
/// With `undo`, the deleted lines are saved for undo first. Stops
/// early if the buffer becomes empty or the last line is reached.
///
/// # Safety
/// Same as [`deleted_lines_mark`].
pub unsafe fn del_lines(nlines: LinenrT, undo: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let curwin = g.curwin;
    let curbuf = g.curbuf;
    // SAFETY: forwarded from this function's own safety doc.
    let first = unsafe { &*curwin }.w_cursor.lnum;

    if nlines <= 0 {
        return;
    }

    // Save the deleted lines for undo.
    // SAFETY: forwarded from this function's own safety doc.
    if undo && unsafe { crate::undo::u_savedel(first, nlines) } == crate::vim_defs::FAIL {
        return;
    }

    let mut n: LinenrT = 0;
    while n < nlines {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { &*curbuf }.b_ml.ml_flags & crate::memline_defs::ML_EMPTY != 0 {
            // Nothing to delete.
            break;
        }

        // SAFETY: forwarded from this function's own safety doc.
        // (the original ignores ml_delete_flags's return value here too.)
        let _ = unsafe { crate::memline::ml_delete_flags(first, crate::memline::ML_DEL_MESSAGE) };
        n += 1;

        // If we delete the last line in the file, stop.
        // SAFETY: forwarded from this function's own safety doc.
        if first > unsafe { &*curbuf }.b_ml.ml_line_count {
            break;
        }
    }

    // Correct the cursor position before calling deleted_lines_mark(),
    // since it may trigger a callback to display the cursor.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *curwin }.w_cursor.col = 0;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::cursor::check_cursor_lnum(curwin) };

    // Adjust marks, mark the buffer as changed and prepare for
    // displaying.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { deleted_lines_mark(first, n) };
}

/// Insert the NUL-terminated byte string `p` at the cursor position
/// (`ins_bytes`).
///
/// # Safety
/// Same as [`ins_char_bytes`].
pub unsafe fn ins_bytes(p: &[u8]) {
    // The original takes a NUL-terminated `char *` and measures it
    // with strlen; here the slice may or may not carry a trailing NUL,
    // so stop at the first one if present.
    let len = p.iter().position(|&b| b == 0).unwrap_or(p.len());
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ins_bytes_len(&p[..len]) };
}

/// Insert the byte string `p` at the cursor position, one whole
/// character at a time (`ins_bytes_len`).
///
/// Handles Replace mode and multi-byte characters, since each
/// character is handed to [`ins_char_bytes`] individually.
///
/// # Safety
/// Same as [`ins_char_bytes`].
pub unsafe fn ins_bytes_len(p: &[u8]) {
    let len = p.len();
    let mut i = 0usize;
    while i < len {
        // Avoid reading past the end of `p`.
        // SAFETY: forwarded from this function's own safety doc.
        let n = unsafe { crate::mbyte::utfc_ptr2len_len(&p[i..], len - i) } as usize;
        if n == 0 {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { ins_char_bytes(&p[i..i + n]) };
        i += n;
    }
}

/// Insert the character `c` at the cursor position (`ins_char`).
///
/// # Safety
/// Same as [`ins_char_bytes`].
pub unsafe fn ins_char(c: i32) {
    let mut buf = [0u8; crate::mbyte_defs::MB_MAXCHAR + 1];
    let n = crate::mbyte::utf_char2bytes(c, &mut buf) as usize;

    // When "c" is 0x100, 0x200, etc. we don't want to insert a NUL
    // byte. Happens for CTRL-Vu9900.
    if buf[0] == 0 {
        buf[0] = b'\n';
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ins_char_bytes(&buf[..n]) };
}

/// Insert (or, in Replace mode, overwrite with) the character whose
/// bytes are `buf`, at the cursor position (`ins_char_bytes`).
///
/// # Scope
///
/// The insert path is translated in full. Two branches are
/// `unimplemented!()`, both behind real guards that are unreachable
/// today rather than hardcoded away:
///
/// The `State & REPLACE_FLAG` block needs `replace_push`/
/// `replace_push_nul` (the Replace-mode undo stack) and, for
/// `VREPLACE_FLAG`, the virtual-replace column accounting. Nothing
/// translated can enter Replace mode - there is no `edit()` loop yet -
/// so `State` never carries that flag in a real session.
///
/// The `'showmatch'` block needs `showmatch` (a `search.c` display
/// routine). `p_sm` is `0` for every session this crate can build, so
/// the guard's own first operand is false.
///
/// # Safety
/// Same as [`ins_str`].
pub unsafe fn ins_char_bytes(buf: &[u8]) {
    let charlen = buf.len();
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;

    // Break tabs if needed.
    // SAFETY: forwarded from this function's own safety doc.
    if crate::state::virtual_active(unsafe { &*curwin }) && unsafe { &*curwin }.w_cursor.coladd > 0
    {
        // SAFETY: forwarded from this function's own safety doc.
        let vis = unsafe { crate::cursor::getviscol() };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::cursor::coladvance_force(vis) };
    }

    // SAFETY: forwarded from this function's own safety doc.
    let col = unsafe { &*curwin }.w_cursor.col as usize;
    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { &*curwin }.w_cursor.lnum;
    // SAFETY: forwarded from this function's own safety doc.
    let oldp = unsafe { crate::memline::ml_get(lnum) };
    // Length of the old line, including its NUL.
    // SAFETY: forwarded from this function's own safety doc.
    let linelen = unsafe { crate::memline::ml_get_len(lnum) } as usize + 1;

    // The lengths default to the values for when not replacing:
    // `oldlen` bytes deleted (0), `newlen` bytes inserted.
    let oldlen = 0usize;
    let newlen = charlen;

    // SAFETY: forwarded from this function's own safety doc.
    let state = unsafe { crate::globals::GLOBALS.get_mut() }.State as u32;
    if state & crate::state_defs::mode::REPLACE_FLAG != 0 {
        unimplemented!(
            "Replace mode needs replace_push/replace_push_nul, not yet translated; \
             unreachable while nothing can enter Replace mode"
        );
    }

    let mut newp = Vec::with_capacity(linelen + newlen - oldlen);
    // Copy bytes before the cursor.
    if col > 0 {
        newp.extend_from_slice(&oldp[..col]);
    }
    // Insert or overwrite the new character.
    newp.extend_from_slice(buf);
    // Fill with spaces when necessary (only ever in Replace mode).
    newp.resize(newp.len() + newlen.saturating_sub(charlen), b' ');
    // Copy the bytes after the changed character(s).
    if linelen > col + oldlen {
        newp.extend_from_slice(&oldp[col + oldlen..linelen]);
    }

    // Replace the line in the buffer.
    // SAFETY: forwarded from this function's own safety doc.
    let _ = unsafe { crate::memline::ml_replace(lnum, &newp) };

    // Mark the buffer as changed and prepare for displaying.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { inserted_bytes(lnum, col as ColnrT, oldlen as i32, newlen as i32) };

    // If we're in Insert or Replace mode and 'showmatch' is set, then
    // briefly show the match for right parens and braces.
    // SAFETY: reading `'showmatch'`.
    let sm = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sm;
    // SAFETY: forwarded from this function's own safety doc.
    let msg_silent = unsafe { crate::globals::GLOBALS.get_mut() }.msg_silent;
    if sm != 0
        && state & crate::state_defs::mode::INSERT != 0
        && msg_silent == 0
        // SAFETY: forwarded from this function's own safety doc.
        && !unsafe { crate::insexpand::ins_compl_active() }
    {
        unimplemented!(
            "'showmatch' needs search.c's showmatch, not yet translated; \
             unreachable while 'showmatch' is off"
        );
    }

    // SAFETY: reading `'revins'`.
    let ri = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ri;
    if ri == 0 || state & crate::state_defs::mode::REPLACE_FLAG != 0 {
        // Normal insert: move the cursor right.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *curwin }.w_cursor.col += charlen as ColnrT;
    }
}

/// Insert the bytes of `s` at the cursor position (`ins_str`).
///
/// The cursor is advanced past the inserted text.
///
/// # Safety
/// Same as [`del_bytes`].
pub unsafe fn ins_str(s: &[u8]) {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { &*curwin }.w_cursor.lnum;

    // SAFETY: forwarded from this function's own safety doc.
    if crate::state::virtual_active(unsafe { &*curwin }) && unsafe { &*curwin }.w_cursor.coladd > 0
    {
        // SAFETY: forwarded from this function's own safety doc.
        let vis = unsafe { crate::cursor::getviscol() };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::cursor::coladvance_force(vis) };
    }

    // SAFETY: forwarded from this function's own safety doc.
    let col = unsafe { &*curwin }.w_cursor.col;
    // SAFETY: forwarded from this function's own safety doc.
    let oldp = unsafe { crate::memline::ml_get(lnum) };
    // SAFETY: forwarded from this function's own safety doc.
    let oldlen = unsafe { crate::memline::ml_get_len(lnum) };

    let slen = s.len();
    let mut newp = Vec::with_capacity(oldlen as usize + slen + 1);
    if col > 0 {
        newp.extend_from_slice(&oldp[..col as usize]);
    }
    newp.extend_from_slice(s);
    // `bytes` covers the rest of the line INCLUDING its trailing NUL.
    let bytes = oldlen - col + 1;
    debug_assert!(bytes >= 0);
    newp.extend_from_slice(&oldp[col as usize..(col + bytes) as usize]);

    // SAFETY: forwarded from this function's own safety doc.
    let _ = unsafe { crate::memline::ml_replace(lnum, &newp) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { inserted_bytes(lnum, col, 0, slen as i32) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *curwin }.w_cursor.col += slen as ColnrT;
}

/// Delete from the cursor position to the end of the line
/// (`truncate_line`).
///
/// With `fixpos`, the cursor is stepped back so it does not end up on
/// the trailing NUL.
///
/// # Safety
/// Same as [`del_bytes`].
pub unsafe fn truncate_line(fixpos: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { &*curwin }.w_cursor.lnum;
    // SAFETY: forwarded from this function's own safety doc.
    let col = unsafe { &*curwin }.w_cursor.col;
    // SAFETY: forwarded from this function's own safety doc.
    let old_line = unsafe { crate::memline::ml_get(lnum) };
    // The original builds either an empty string or the first `col`
    // bytes; both need their own trailing NUL, which `ml_replace`
    // expects to be part of the slice in this crate.
    let mut newp = Vec::with_capacity(col as usize + 1);
    if col > 0 {
        newp.extend_from_slice(&old_line[..col as usize]);
    }
    newp.push(0);
    // SAFETY: forwarded from this function's own safety doc.
    let deleted = unsafe { crate::memline::ml_get_len(lnum) } - col;

    // SAFETY: forwarded from this function's own safety doc.
    let _ = unsafe { crate::memline::ml_replace(lnum, &newp) };

    // Mark the buffer as changed and prepare for displaying.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { inserted_bytes(lnum, col, deleted, 0) };

    // If "fixpos" is true we don't want to end up positioned at the NUL.
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *curwin };
    if fixpos && w.w_cursor.col > 0 {
        w.w_cursor.col -= 1;
    }
}

/// Delete one character under the cursor (`del_char`).
///
/// Returns `FAIL` when the cursor sits on the NUL past the end of the
/// line. Caller must have prepared for undo.
///
/// # Safety
/// Same as [`del_bytes`].
pub unsafe fn del_char(fixpos: bool) -> i32 {
    // Make sure the cursor is at the start of a character.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::mbyte::mb_adjust_cursor() };
    // SAFETY: forwarded from this function's own safety doc.
    let p = unsafe { crate::cursor::get_cursor_pos_ptr() };
    if p.first().copied().unwrap_or(0) == 0 {
        return crate::vim_defs::FAIL;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { del_chars(1, fixpos) }
}

/// Like [`del_bytes`], but deletes characters instead of bytes
/// (`del_chars`).
///
/// # Safety
/// Same as [`del_bytes`].
pub unsafe fn del_chars(count: i32, fixpos: bool) -> i32 {
    let mut bytes = 0;
    // SAFETY: forwarded from this function's own safety doc.
    let p = unsafe { crate::cursor::get_cursor_pos_ptr() };
    let mut idx = 0usize;
    let mut i = 0;
    while i < count && p.get(idx).copied().unwrap_or(0) != 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let l = unsafe { crate::mbyte::utfc_ptr2len(&p[idx..]) };
        bytes += l;
        idx += l as usize;
        i += 1;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { del_bytes(bytes, fixpos, true) }
}

/// Delete `count` bytes at the cursor position (`del_bytes`).
///
/// Returns `FAIL` when the cursor sits on the NUL past the end of the
/// line, or when `count` is negative; `OK` otherwise (including for a
/// zero `count`, which does nothing).
///
/// With `use_delcombine` and `'delcombine'` set, deleting less than one
/// whole character instead removes only the last combining character.
/// `fixpos` keeps the cursor off the trailing NUL when the last
/// character of a non-blank line is removed.
///
/// The original's `siemsg("E292: ...")` for a negative count is
/// omitted, matching this crate's established "skip the deferred
/// message-display side effect, keep the exact same pass/fail
/// outcome" policy (`arglist::check_arglist_locked`,
/// `window::check_split_disallowed`).
///
/// # Safety
/// `GLOBALS.curwin`/`curbuf` must be valid and the buffer must have a
/// live memline; same as [`inserted_bytes`].
pub unsafe fn del_bytes(count: ColnrT, fixpos_arg: bool, use_delcombine: bool) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let curwin = g.curwin;
    let curbuf = g.curbuf;
    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { &*curwin }.w_cursor.lnum;
    // SAFETY: forwarded from this function's own safety doc.
    let mut col = unsafe { &*curwin }.w_cursor.col;
    let mut fixpos = fixpos_arg;
    // SAFETY: forwarded from this function's own safety doc.
    let oldp = unsafe { crate::memline::ml_get(lnum) };
    // SAFETY: forwarded from this function's own safety doc.
    let oldlen = unsafe { crate::memline::ml_get_len(lnum) };
    let mut count = count;

    // Can't do anything when the cursor is on the NUL after the line.
    if col >= oldlen {
        return crate::vim_defs::FAIL;
    }
    // If "count" is zero there is nothing to do.
    if count == 0 {
        return crate::vim_defs::OK;
    }
    // If "count" is negative the caller must be doing something wrong.
    if count < 1 {
        return crate::vim_defs::FAIL;
    }

    // If 'delcombine' is set and we are deleting (less than) one
    // character, only delete the last combining character.
    // SAFETY: reading `'delcombine'`.
    let deco = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_deco;
    // SAFETY: forwarded from this function's own safety doc.
    if deco != 0
        && use_delcombine
        && unsafe { crate::mbyte::utfc_ptr2len(&oldp[col as usize..]) } >= count
    {
        let mut state = crate::mbyte_defs::GRAPHEME_STATE_INIT;
        let first_len = crate::mbyte::utf_ptr2len(&oldp[col as usize..]);
        // SAFETY: forwarded from this function's own safety doc.
        let composing = unsafe {
            crate::mbyte::utf_composinglike(
                &oldp[col as usize..],
                &oldp[(col + first_len) as usize..],
                &mut state,
            )
        };
        if composing {
            // Find the last composing char; there can be several.
            let mut n = col;
            loop {
                col = n;
                count = crate::mbyte::utf_ptr2len(&oldp[n as usize..]);
                n += count;
                // SAFETY: forwarded from this function's own safety doc.
                let more = unsafe {
                    crate::mbyte::utf_composinglike(
                        &oldp[col as usize..],
                        &oldp[n as usize..],
                        &mut state,
                    )
                };
                if !more {
                    break;
                }
            }
            fixpos = false;
        }
    }

    // When count is too big, reduce it. `movelen` includes the
    // trailing NUL.
    let mut movelen = oldlen - col - count + 1;
    if movelen <= 1 {
        // If we just took off the last character of a non-blank line,
        // and fixpos is true, we don't want to end up positioned at
        // the NUL - unless "restart_edit" is set or 'virtualedit'
        // contains "onemore".
        // SAFETY: forwarded from this function's own safety doc.
        let restart_edit = unsafe { crate::globals::GLOBALS.get_mut() }.restart_edit;
        // SAFETY: forwarded from this function's own safety doc.
        let ve = crate::option::get_ve_flags(unsafe { &*curwin });
        if col > 0 && fixpos && restart_edit == 0 && ve & crate::option_vars::opt_ve_flag::ONEMORE == 0
        {
            // SAFETY: forwarded from this function's own safety doc.
            let w = unsafe { &mut *curwin };
            w.w_cursor.col -= 1;
            w.w_cursor.coladd = 0;
            // SAFETY: forwarded from this function's own safety doc.
            w.w_cursor.col -= unsafe { crate::mbyte::utf_head_off(&oldp, w.w_cursor.col as usize) };
        }
        count = oldlen - col;
        movelen = 1;
    }
    let newlen = oldlen - count;

    // Build the line with the deleted range removed. The original
    // either edits the memline's own allocation in place (when the
    // line is already dirty) or allocates a fresh line; this crate's
    // `ml_get` hands back an owned copy either way, so the new content
    // is assembled once and then stored through whichever path the
    // original would have taken.
    let mut newp = Vec::with_capacity((newlen + 1) as usize);
    newp.extend_from_slice(&oldp[..col as usize]);
    newp.extend_from_slice(&oldp[(col + count) as usize..(col + count + movelen) as usize]);

    // SAFETY: forwarded from this function's own safety doc.
    let alloc_newp = !unsafe { crate::memline::ml_line_alloced() };
    if alloc_newp {
        // SAFETY: forwarded from this function's own safety doc.
        // (the original ignores ml_replace's return value here too.)
        let _ = unsafe { crate::memline::ml_replace(lnum, &newp) };
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        let b = unsafe { &mut *curbuf };
        if let Some(ptr) = b.b_ml.ml_line_ptr.as_deref() {
            crate::memline::ml_add_deleted_len(ptr, Some(oldlen as usize));
        }
        b.b_ml.ml_line_ptr = Some(newp);
        b.b_ml.ml_line_textlen = newlen + 1;
    }

    // Mark the buffer as changed and prepare for displaying.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { inserted_bytes(lnum, col, count, 0) };

    crate::vim_defs::OK
}

/// Insert or delete bytes at a column (`inserted_bytes`).
///
/// Like [`changed_bytes`], but also adjusts extmarks for the "new"
/// bytes.
///
/// # Safety
/// Same as `changed_common`.
pub unsafe fn inserted_bytes(lnum: LinenrT, start_col: ColnrT, old_col: i32, new_col: i32) {
    // SAFETY: reading a plain scalar global.
    if *unsafe { crate::extmark::CURBUF_SPLICE_PENDING.get_mut() } == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            crate::extmark::extmark_splice_cols(
                &mut *curbuf,
                lnum - 1,
                start_col,
                old_col,
                new_col,
                crate::extmark_defs::ExtmarkOp::Undo,
            );
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { changed_bytes(lnum, start_col) };
}

/// Changed lines of a buffer (`changed_lines`).
///
/// Must be called AFTER the change and after `mark_adjust()`. `lnum`
/// is the first line that needs displaying, `lnume` the first line
/// below the changed lines (BEFORE the change); when only inserting
/// lines the two are equal. Careful: may trigger autocommands that
/// reload the buffer.
///
/// # Safety
/// Same as `changed_common`.
pub unsafe fn changed_lines(
    buf: *mut BufT,
    lnum: LinenrT,
    col: ColnrT,
    lnume: LinenrT,
    xtra: LinenrT,
    do_buf_event: bool,
) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { changed_lines_redraw_buf(buf, lnum, lnume, xtra) };

    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let curwin = g.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let cur = unsafe { &*curwin };
    if xtra == 0
        && cur.w_onebuf_opt.wo_diff != 0
        && cur.w_buffer == buf
        && !crate::diff::diff_internal()
    {
        // When the number of lines doesn't change then mark_adjust()
        // isn't called, and other diff buffers still need to be marked
        // for displaying.
        let mut wp = g.firstwin;
        while !wp.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            let w = unsafe { &*wp };
            let next = w.w_next;
            if w.w_onebuf_opt.wo_diff != 0 && wp != curwin {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::drawscreen::redraw_later(wp, crate::drawscreen::UPD_VALID) };
                // SAFETY: forwarded from this function's own safety doc.
                let wlnum = unsafe { crate::diff::diff_lnum_win(lnum, wp) };
                if wlnum > 0 {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe {
                        changed_lines_redraw_buf(w.w_buffer, wlnum, lnume - lnum + wlnum, 0);
                    }
                }
            }
            wp = next;
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { changed_common(buf, lnum, col, lnume, xtra) };

    if do_buf_event {
        let num_added = i64::from(lnume + xtra - lnum);
        let num_removed = i64::from(lnume - lnum);
        // SAFETY: forwarded from this function's own safety doc.
        crate::buffer_updates::buf_updates_send_changes(
            unsafe { &mut *buf },
            lnum,
            num_added,
            num_removed,
        );
    }
}

/// Appended `count` lines below line `lnum` in `buf`
/// (`appended_lines_buf`).
///
/// Must be called AFTER the change and after `mark_adjust()`.
///
/// # Safety
/// Same as [`changed_lines`].
pub unsafe fn appended_lines_buf(buf: *mut BufT, lnum: LinenrT, count: LinenrT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { changed_lines(buf, lnum + 1, 0, lnum + 1, count, true) };
}

/// Appended `count` lines below line `lnum` in the current buffer
/// (`appended_lines`).
///
/// # Safety
/// Same as [`changed_lines`].
pub unsafe fn appended_lines(lnum: LinenrT, count: LinenrT) {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { appended_lines_buf(curbuf, lnum, count) };
}

/// Like [`appended_lines`], but adjusts marks first
/// (`appended_lines_mark`).
///
/// # Safety
/// Same as [`changed_lines`], plus `crate::mark::mark_adjust`'s own.
pub unsafe fn appended_lines_mark(lnum: LinenrT, count: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        crate::mark::mark_adjust(
            lnum + 1,
            crate::pos_defs::MAXLNUM,
            count,
            0,
            crate::extmark_defs::ExtmarkOp::Undo,
        );
    }
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { changed_lines(curbuf, lnum + 1, 0, lnum + 1, count, true) };
}

/// Deleted `count` lines at line `lnum` in `buf` (`deleted_lines_buf`).
///
/// Must be called AFTER the change and after `mark_adjust()`.
///
/// # Safety
/// Same as [`changed_lines`].
pub unsafe fn deleted_lines_buf(buf: *mut BufT, lnum: LinenrT, count: LinenrT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { changed_lines(buf, lnum, 0, lnum + count, -count, true) };
}

/// Deleted `count` lines at line `lnum` in the current buffer
/// (`deleted_lines`).
///
/// # Safety
/// Same as [`changed_lines`].
pub unsafe fn deleted_lines(lnum: LinenrT, count: LinenrT) {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { deleted_lines_buf(curbuf, lnum, count) };
}

/// Like [`deleted_lines`], but adjusts marks first
/// (`deleted_lines_mark`).
///
/// Make sure the cursor is on a valid line before calling: a GUI
/// callback may be triggered to display the cursor.
///
/// # Safety
/// Same as [`changed_lines`], plus `crate::mark::mark_adjust`'s and
/// `crate::extmark::extmark_adjust`'s own.
pub unsafe fn deleted_lines_mark(lnum: LinenrT, count: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
    // SAFETY: forwarded from this function's own safety doc.
    let made_empty =
        count > 0 && unsafe { &*curbuf }.b_ml.ml_flags & crate::memline_defs::ML_EMPTY != 0;

    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        crate::mark::mark_adjust(
            lnum,
            lnum + count - 1,
            crate::pos_defs::MAXLNUM,
            -count,
            crate::extmark_defs::ExtmarkOp::Noop,
        );
    }
    // If we deleted the entire buffer we need to implicitly add a new
    // empty line.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        crate::extmark::extmark_adjust(
            &mut *curbuf,
            lnum,
            lnum + count - 1,
            crate::pos_defs::MAXLNUM,
            -count + i32::from(made_empty),
            crate::extmark_defs::ExtmarkOp::Undo,
        );
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { changed_lines(curbuf, lnum, 0, lnum + count, -count, true) };
}

/// Changed bytes within a single line of the current buffer
/// (`changed_bytes`).
///
/// Marks the windows on this buffer to be redisplayed, marks the
/// buffer changed via [`changed`], and invalidates cached values.
/// Careful: may trigger autocommands that reload the buffer.
///
/// # Safety
/// Same as `changed_common`.
pub unsafe fn changed_bytes(lnum: LinenrT, col: ColnrT) {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let curbuf = g.curbuf;
    let curwin = g.curwin;

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { changed_lines_redraw_buf(curbuf, lnum, lnum + 1, 0) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { changed_common(curbuf, lnum, col, lnum + 1, 0) };

    // When text has been changed at the end of the line, possibly the
    // start of the next line may have SpellCap that should be removed,
    // or it needs to be displayed. Schedule the next line for
    // redrawing just in case. Don't do this when displaying '$' at the
    // end of changed text.
    // SAFETY: forwarded from this function's own safety doc.
    let spell = unsafe { crate::spell::spell_check_window(&*curwin) };
    // SAFETY: forwarded from this function's own safety doc.
    let line_count = unsafe { &*curbuf }.b_ml.ml_line_count;
    if spell && lnum < line_count {
        // SAFETY: reading `'cpoptions'`, matching this crate's
        // established `GlobalCell::get_mut` convention.
        let cpo = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo.clone();
        let has_dollar = cpo.as_deref().is_some_and(|c| {
            vim_strchr(c, i32::from(crate::option_vars::CPO_DOLLAR)).is_some()
        });
        if !has_dollar {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::drawscreen::redraw_win_line(curwin, lnum + 1) };
        }
    }

    // Notify any channels that are watching.
    // SAFETY: forwarded from this function's own safety doc.
    crate::buffer_updates::buf_updates_send_changes(unsafe { &mut *curbuf }, lnum, 1, 1);

    // Diff highlighting in other diff windows may need updating too.
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { &*curwin }.w_onebuf_opt.wo_diff == 0 {
        return;
    }
    let mut wp = g.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let w = unsafe { &*wp };
        let next = w.w_next;
        if w.w_onebuf_opt.wo_diff != 0 && wp != curwin {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::drawscreen::redraw_later(wp, crate::drawscreen::UPD_VALID) };
            // SAFETY: forwarded from this function's own safety doc.
            let wlnum = unsafe { crate::diff::diff_lnum_win(lnum, wp) };
            if wlnum > 0 {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { changed_lines_redraw_buf(w.w_buffer, wlnum, wlnum + 1, 0) };
            }
        }
        wp = next;
    }
}

/// Common code for when a change was made (`changed_common`).
///
/// See `changed_lines()` for the arguments. Careful: may trigger
/// autocommands that reload the buffer.
///
/// # Scope
///
/// Translated in full. Every dependency is real: `diff_internal`,
/// `diff_update_line`, `mark_view_make`, `comp_textwidth`,
/// `check_visual_pos`, `linetabsize_eol`, `sms_marker_overlap`,
/// `fold_update`, `has_folding_win`, `has_any_folding`, `set_topline`,
/// `redraw_later`, `set_must_redraw` and
/// [`changed_lines_invalidate_win`].
///
/// The original's `FOR_ALL_WINDOWS_IN_TAB`/`FOR_ALL_TAB_WINDOWS`
/// macros both walk `GLOBALS.firstwin`/`w_next` here, matching the
/// precedent already set by `drawscreen.rs`'s own
/// `redraw_buf_status_later`.
///
/// # Safety
/// `buf` must be a valid, non-null pointer to a live `BufT`, and
/// `GLOBALS.firstwin`'s own `w_next` chain, `GLOBALS.curwin` and
/// `GLOBALS.curtab` must all be valid.
unsafe fn changed_common(
    buf: *mut BufT,
    lnum: LinenrT,
    col: ColnrT,
    lnume: LinenrT,
    xtra: LinenrT,
) {
    // Mark the buffer as modified.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { changed(buf) };

    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let curwin = g.curwin;
    let curtab = g.curtab;
    let firstwin = g.firstwin;

    let mut wp = firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let w = unsafe { &*wp };
        let next = w.w_next;
        if w.w_buffer == buf && w.w_onebuf_opt.wo_diff != 0 && crate::diff::diff_internal() {
            // SAFETY: forwarded from this function's own safety doc.
            // (`tp_diff_update` is an `int` in the original, so this
            // assigns 1 rather than a `bool`.)
            unsafe { &mut *curtab }.tp_diff_update = 1;
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::diff::diff_update_line(lnum) };
        }
        wp = next;
    }

    // Set the '. mark.
    if g.cmdmod.cmod_flags & crate::ex_cmds_defs::cmod::KEEPJUMPS == 0 {
        // Set the mark view only if lnum is visible, since changes
        // might be made outside of the current window's view.
        let mut view = crate::mark_defs::FmarkvT::default();
        // SAFETY: forwarded from this function's own safety doc.
        let cur = unsafe { &*curwin };
        if cur.w_buffer == buf && lnum >= cur.w_topline && lnum <= cur.w_botline {
            view = crate::mark::mark_view_make(cur, cur.w_cursor);
        }
        // SAFETY: forwarded from this function's own safety doc.
        let b = unsafe { &mut *buf };
        let handle = b.handle;
        crate::mark::reset_fmark(
            &mut b.b_last_change,
            crate::pos_defs::PosT { lnum, col, coladd: 0 },
            handle,
            view,
        );

        // Create a new entry if a new undo-able change was started, or
        // if we don't have an entry yet.
        if b.b_new_change || b.b_changelistlen == 0 {
            let add = if b.b_changelistlen == 0 {
                true
            } else {
                // Don't create a new entry when the line number is the
                // same as the last one and the column is not too far
                // away. Avoids creating many entries for typing
                // "xxxxx".
                let p = b.b_changelist[(b.b_changelistlen - 1) as usize].mark;
                if p.lnum != lnum {
                    true
                } else {
                    // SAFETY: forwarded from this function's own safety doc.
                    let mut cols = unsafe { crate::textformat::comp_textwidth(false) };
                    if cols == 0 {
                        cols = 79;
                    }
                    p.col + cols < col || col + cols < p.col
                }
            };
            if add {
                // This is the first of a new sequence of undo-able
                // changes and it's at some distance from the last
                // change, so use a new position in the changelist.
                b.b_new_change = false;

                if b.b_changelistlen == crate::mark_defs::JUMPLISTSIZE {
                    // Changelist is full: remove the oldest entry.
                    b.b_changelistlen = crate::mark_defs::JUMPLISTSIZE - 1;
                    b.b_changelist.rotate_left(1);
                    let mut wp = firstwin;
                    while !wp.is_null() {
                        // SAFETY: forwarded from this function's own safety doc.
                        let w = unsafe { &mut *wp };
                        // Correct the position in the changelist for
                        // other windows on this buffer.
                        if w.w_buffer == buf && w.w_changelistidx > 0 {
                            w.w_changelistidx -= 1;
                        }
                        wp = w.w_next;
                    }
                }
                let mut wp = firstwin;
                while !wp.is_null() {
                    // SAFETY: forwarded from this function's own safety doc.
                    let w = unsafe { &mut *wp };
                    // For other windows, if the position in the
                    // changelist is at the end it stays at the end.
                    if w.w_buffer == buf && w.w_changelistidx == b.b_changelistlen {
                        w.w_changelistidx += 1;
                    }
                    wp = w.w_next;
                }
                b.b_changelistlen += 1;
            }
        }
        b.b_changelist[(b.b_changelistlen - 1) as usize] = b.b_last_change.clone();
        // The current window is always after the last change, so that
        // "g," takes you back to it.
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { &*curwin }.w_buffer == buf {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &mut *curwin }.w_changelistidx = b.b_changelistlen;
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { &*curwin }.w_buffer == buf && g.Visual.active {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::cursor::check_visual_pos() };
    }

    let mut wp = firstwin;
    while !wp.is_null() {
        // Each `&mut *wp` below is deliberately short-lived and
        // re-derived after every call that itself reborrows `wp`.
        // Holding one long-lived `w` across such a call would
        // invalidate it under Tree Borrows, which Miri rejects.
        // SAFETY: forwarded from this function's own safety doc.
        let next = unsafe { &*wp }.w_next;
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { &*wp }.w_buffer != buf {
            wp = next;
            continue;
        }

        // Mark this window to be redrawn later.
        // SAFETY: reading a plain `bool` global.
        let not_allowed = *unsafe { crate::drawscreen::REDRAW_NOT_ALLOWED.get_mut() };
        {
            // SAFETY: forwarded from this function's own safety doc.
            let w = unsafe { &mut *wp };
            if !not_allowed && w.w_redr_type < crate::drawscreen::UPD_VALID {
                w.w_redr_type = crate::drawscreen::UPD_VALID;
            }
        }

        // When inserting/deleting lines and the window has specific
        // lines to be redrawn, w_redraw_top and w_redraw_bot may now
        // be invalid, so just redraw everything.
        // SAFETY: forwarded from this function's own safety doc.
        if xtra != 0 && unsafe { &*wp }.w_redraw_top != 0 {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::drawscreen::redraw_later(wp, crate::drawscreen::UPD_NOT_VALID) };
        }

        // Last line after the change.
        let mut last = lnume + xtra - 1;

        // Reset "w_skipcol" if the topline length has become so much
        // smaller that nothing will be visible anymore, accounting for
        // 'smoothscroll' <<< or the 'listchars' "precedes" marker.
        // SAFETY: forwarded from this function's own safety doc.
        let (skipcol, topline) = {
            let w = unsafe { &*wp };
            (w.w_skipcol, w.w_topline)
        };
        if skipcol > 0 {
            let topline_shrank = last < topline || {
                if topline >= lnum && topline < lnume {
                    // SAFETY: forwarded from this function's own safety doc.
                    let width = unsafe { crate::plines::linetabsize_eol(wp, topline) };
                    // SAFETY: forwarded from this function's own safety doc.
                    let overlap = unsafe { crate::r#move::sms_marker_overlap(&mut *wp, -1) };
                    width <= skipcol + overlap
                } else {
                    false
                }
            };
            if topline_shrank {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { &mut *wp }.w_skipcol = 0;
            }
        }

        // Check if a change in the buffer has invalidated the cached
        // values for the cursor, and update the folds for this window.
        // Can't postpone this, because a following operator might work
        // on the whole fold: ">>dd".
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::fold::fold_update(&mut *wp, lnum, last) };

        // The change may cause lines above or below it to become
        // included in a fold. Set lnum/lnume to the first/last line
        // that might be displayed differently. Setting w_cline_folded
        // here is an efficient way to update it when inserting lines
        // just above a closed fold.
        let mut lnum = lnum;
        // SAFETY: forwarded from this function's own safety doc.
        let folded = unsafe {
            crate::fold::has_folding_win(&mut *wp, lnum, Some(&mut lnum), None, false, None)
        };
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { &*wp }.w_cursor.lnum == lnum {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &mut *wp }.w_cline_folded = folded;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let folded = unsafe {
            crate::fold::has_folding_win(&mut *wp, last, None, Some(&mut last), false, None)
        };
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { &*wp }.w_cursor.lnum == last {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &mut *wp }.w_cline_folded = folded;
        }

        // SAFETY: forwarded from this function's own safety doc.
        unsafe { changed_lines_invalidate_win(wp, lnum, col, lnume, xtra) };

        // Take care of side effects for setting w_topline when folds
        // have changed. Especially when the buffer was changed in
        // another window.
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::fold::has_any_folding(&*wp) } {
            // SAFETY: forwarded from this function's own safety doc.
            let topline = unsafe { &*wp }.w_topline;
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::r#move::set_topline(wp, topline) };
        }

        {
            // SAFETY: forwarded from this function's own safety doc.
            let w = unsafe { &mut *wp };

            // If lines have been added or removed, relative numbering
            // always requires an update even if the cursor didn't move.
            if w.w_onebuf_opt.wo_rnu != 0 && xtra != 0 {
                w.w_last_cursor_lnum_rnu = 0;
            }

            if w.w_onebuf_opt.wo_cul != 0 && w.w_last_cursorline >= lnum {
                if w.w_last_cursorline < lnume {
                    // If 'cursorline' was inside the change, it has
                    // already been invalidated in w_lines[] by the
                    // loop above.
                    w.w_last_cursorline = 0;
                } else {
                    // If 'cursorline' was below the change, adjust its
                    // lnum.
                    w.w_last_cursorline += xtra;
                }
            }
        }

        if wp == curwin && xtra != 0 {
            // SAFETY: reading a plain scalar global.
            let has = unsafe { crate::drawscreen::SEARCH_HL_HAS_CURSOR_LNUM.get_mut() };
            if *has >= lnum {
                *has += xtra;
            }
        }

        wp = next;
    }

    // Call update_screen() later, which checks out what needs to be
    // redrawn, since it notices b_mod_set and then uses b_mod_*.
    crate::drawscreen::set_must_redraw(crate::drawscreen::UPD_VALID);

    // When the cursor line is changed always trigger CursorMoved.
    // SAFETY: reading a plain pointer global.
    let last_win = *unsafe { crate::autocmd::LAST_CURSORMOVED_WIN.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let cur = unsafe { &*curwin };
    if last_win == curwin
        && cur.w_buffer == buf
        && lnum <= cur.w_cursor.lnum
        && lnume + xtra.abs() > cur.w_cursor.lnum
    {
        // SAFETY: as above.
        unsafe { crate::autocmd::LAST_CURSORMOVED.get_mut() }.lnum = 0;
    }
}

/// Record that lines `lnum..lnume` of `buf` changed, so that
/// `win_update()` redisplays them (`changed_lines_redraw_buf`).
///
/// `xtra` is the number of lines added (positive) or removed
/// (negative) by the change. Repeated calls widen the pending
/// `b_mod_*` region rather than replacing it.
///
/// # Safety
/// `buf` must be a valid, non-null pointer to a live `BufT` whose
/// marktree is well-formed.
pub unsafe fn changed_lines_redraw_buf(
    buf: *mut BufT,
    lnum: LinenrT,
    lnume: LinenrT,
    xtra: LinenrT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let b = unsafe { &mut *buf };
    let mut lnume = lnume;

    // If lines have been deleted and there may be decorations in the
    // buffer, ensure win_update() calculates the height of, and
    // redraws, the line to which (or whence) a mark may have moved.
    // When lines are deleted a virt_line mark may be drawn two lines
    // below, so increase by one more.
    if xtra != 0 && b.b_marktree.n_keys > 0 {
        let has_virt_lines =
            xtra < 0 && crate::buffer::buf_meta_total(b, crate::marktree_defs::MetaIndex::Lines) != 0;
        lnume += 1 + LinenrT::from(has_virt_lines);
    }

    if b.b_mod_set {
        // Find the maximum area that must be redisplayed.
        b.b_mod_top = b.b_mod_top.min(lnum);
        if lnum < b.b_mod_bot {
            // Adjust the old bottom position for the extra lines.
            b.b_mod_bot += xtra;
            b.b_mod_bot = b.b_mod_bot.max(lnum);
        }
        b.b_mod_bot = b.b_mod_bot.max(lnume + xtra);
        b.b_mod_xlines += xtra;
    } else {
        // Set the area that must be redisplayed.
        b.b_mod_set = true;
        b.b_mod_top = lnum;
        b.b_mod_bot = lnume + xtra;
        b.b_mod_xlines = xtra;
    }
}

/// Call this function when something in a buffer is changed (`changed`).
/// Most often called through `changed_bytes()`/`changed_lines()` (both
/// still deferred - they also mark the display area to redraw), which
/// also mark the area of the display to be redrawn.
///
/// Careful: may trigger autocommands that reload the buffer (via
/// [`change_warning`]).
///
/// # Safety
/// Same as [`change_warning`]/[`changed_internal`].
pub unsafe fn changed(buf: *mut BufT) {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { (*buf).b_changed } == 0 {
        // Give a warning about changing a read-only file. This may
        // also check-out the file, thus change "curbuf"!
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { change_warning(buf, 0) };

        // Create a swap file if that is wanted (not for "nofile"/
        // "nowrite" buffer types). `buf.b_may_swap` is always false
        // today - nothing bootstraps real option defaults for
        // `'updatecount'`/`'swapfile'` yet (matching this crate's own
        // established "`OPTION_VARS` defaults to raw C zero-init, not
        // the real post-startup value" convention - see `ml_open`'s
        // own `b_may_swap` assignment, which needs `p_uc != 0`).
        // This real, always-false-today check is kept (not hardcoded
        // away): its own body `unimplemented!()`s if ever genuinely
        // reached, needing `ml_open_file` (real swap-file creation)
        // plus the message-display pipeline, neither translated.
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { (*buf).b_may_swap } && !crate::buffer::bt_dontwrite(unsafe { Some(&*buf) }) {
            unimplemented!(
                "change::changed: creating a swap file needs ml_open_file \
                 (real file I/O) plus the message-display pipeline, neither \
                 translated - unreachable today since BufT.b_may_swap is \
                 always false, see this function's own doc comment"
            );
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { changed_internal(buf) };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::buffer::buf_inc_changedtick(&mut *buf) };

    // If a pattern is highlighted, the position may now be invalid.
    unsafe { crate::globals::GLOBALS.get_mut() }.Search.hl_match = false;
}

/// Internal part of `changed()`, no user interaction (`changed_internal`).
/// Also used for recovery.
///
/// # Safety
/// `buf` must be a valid, non-null pointer to a live `BufT`.
/// `buf.b_ml.ml_mfp`, if non-null, must be a valid pointer to a live
/// `MemfileT` (touched transitively via `ml_setflags`).
/// `GLOBALS.firstwin`'s own `w_next` chain must consist of valid, live
/// `WinT` pointers (touched transitively via `redraw_buf_status_later`).
pub unsafe fn changed_internal(buf: *mut BufT) {
    // SAFETY: forwarded from this function's own safety doc.
    let was_changed = unsafe { (*buf).b_changed } != 0;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*buf).b_changed = 1 };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::memline::ml_setflags(&mut *buf) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::drawscreen::redraw_buf_status_later(buf) };
    unsafe { crate::globals::GLOBALS.get_mut() }.redraw_tabline = true;
    unsafe { crate::globals::GLOBALS.get_mut() }.need_maketitle = true;
    if !was_changed {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::autocmd::aucmd_defer_modified(buf, true) };
    }
}

/// Called when the changed flag must be reset for buffer `buf`
/// (`unchanged`). When `ff` is true also reset `'fileformat'`. When
/// `always_inc_changedtick` is true, `b:changedtick` is incremented
/// even when the changed flag was off.
///
/// The original's own `file_ff_differs(buf, false)` call always
/// passes `false` for its own `ignore_empty` parameter - meaning this
/// function never needs a real, `ml_open`'d memline to have been
/// opened first (`file_ff_differs`'s only branch that would touch
/// `ml_get_buf` is unconditionally skipped whenever `ignore_empty` is
/// `false`).
///
/// # Safety
/// Same as [`changed_internal`].
pub unsafe fn unchanged(buf: *mut BufT, ff: bool, always_inc_changedtick: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let is_changed = unsafe { (*buf).b_changed } != 0;
    // SAFETY: forwarded from this function's own safety doc.
    let ff_differs = ff && unsafe { file_ff_differs(&mut *buf, false) };
    if is_changed || ff_differs {
        let was_changed = is_changed;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*buf).b_changed = 0 };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::memline::ml_setflags(&mut *buf) };
        if ff {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { save_file_ff(&mut *buf) };
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::drawscreen::redraw_buf_status_later(buf) };
        unsafe { crate::globals::GLOBALS.get_mut() }.redraw_tabline = true;
        unsafe { crate::globals::GLOBALS.get_mut() }.need_maketitle = true;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::buffer::buf_inc_changedtick(&mut *buf) };
        if was_changed {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::autocmd::aucmd_defer_modified(buf, false) };
        }
    } else if always_inc_changedtick {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::buffer::buf_inc_changedtick(&mut *buf) };
    }
}

/// Remember the current values of `'fileformat'`/`'fileencoding'`/
/// `'endofline'`/`'bomb'`, so a later call to [`file_ff_differs`] can
/// detect if they changed (`save_file_ff`).
///
/// Deviates from the original's own "only alloc when the value
/// actually differs" `xfree`/`strcmp`/`xstrdup` dance for
/// `b_start_fenc` by always assigning a fresh clone directly - Rust's
/// own `Option<Vec<u8>>` ownership already makes this observably
/// identical (no caller can detect whether the allocation was reused
/// or fresh), matching this crate's established "idiomatic Rust
/// supersedes a manual C micro-optimization" precedent.
pub fn save_file_ff(buf: &mut BufT) {
    buf.b_start_ffc =
        i32::from(buf.b_p_ff.as_deref().and_then(<[u8]>::first).copied().unwrap_or(0));
    buf.b_start_eof = buf.b_p_eof;
    buf.b_start_eol = buf.b_p_eol;
    buf.b_start_bomb = buf.b_p_bomb;
    buf.b_start_fenc = buf.b_p_fenc.clone();
}

/// Return true if `'fileformat'` and/or `'fileencoding'` has a
/// different value from when editing started (`save_file_ff()`
/// called). Also true when `'endofline'` was changed and `'binary'`
/// is set, or when `'bomb'` was changed and `'binary'` is not set.
/// Also true when `'endofline'` was changed and `'fixeol'` is not set.
/// When `ignore_empty` is true, don't consider a new, empty buffer to
/// be changed (`file_ff_differs`).
///
/// # Safety
/// `buf.b_ml.ml_mfp`, if non-null, must be a valid pointer to a live
/// `MemfileT` (touched transitively via `crate::memline::ml_get_buf`).
#[must_use]
pub unsafe fn file_ff_differs(buf: &mut BufT, ignore_empty: bool) -> bool {
    // In a buffer that was never loaded the options are not valid.
    if buf.b_flags & (b_flags::BF_NEVERLOADED as i32) != 0 {
        return false;
    }
    if ignore_empty
        && buf.b_flags & (b_flags::BF_NEW as i32) != 0
        && buf.b_ml.ml_line_count == 1
        // SAFETY: forwarded from this function's own safety doc.
        && unsafe { crate::memline::ml_get_buf(buf, 1) }.first() == Some(&0)
    {
        return false;
    }
    let ff_first_byte = buf.b_p_ff.as_deref().and_then(<[u8]>::first).copied().unwrap_or(0);
    if buf.b_start_ffc != i32::from(ff_first_byte) {
        return true;
    }
    if (buf.b_p_bin != 0 || buf.b_p_fixeol == 0)
        && (buf.b_start_eof != buf.b_p_eof || buf.b_start_eol != buf.b_p_eol)
    {
        return true;
    }
    if buf.b_p_bin == 0 && buf.b_start_bomb != buf.b_p_bomb {
        return true;
    }
    let Some(start_fenc) = &buf.b_start_fenc else {
        return buf.b_p_fenc.as_deref().is_some_and(|s| !s.is_empty());
    };
    buf.b_p_fenc.as_deref().unwrap_or(b"") != start_fenc.as_slice()
}

/// If the file is readonly, give a warning message with the first
/// change. Don't do this for autocommands. Doesn't use `emsg()`,
/// because it flushes the macro buffer. If we have undone all changes
/// `b_changed` will be false, but `b_did_warn` will be true. `col` is
/// the column for the message; non-zero when in insert mode and
/// `'showmode'` is on.
///
/// Careful: may trigger autocommands that reload the buffer
/// (`change_warning`).
///
/// The real message display (`msg_start`/`msg_source`/`msg_puts_hl`/
/// `msg_clr_eos`/`msg_end`/`msg_delay`/`showmode`) is skipped -
/// `message.c`'s display pipeline is not yet tractable - but every
/// OTHER observable state change is kept: `apply_autocmds` is called
/// for real (currently always a no-op today - see
/// `crate::autocmd`'s own module doc), `v:warningmsg` is set for real
/// via `set_vim_var_string` (it only touches the `VIMVARS` storage
/// slot directly, no `evalvars_init` dict-wiring needed), and
/// `buf.b_did_warn`/`GLOBALS.redraw_cmdline` are still set exactly as
/// the original does.
///
/// `buf` is a raw pointer (not `&mut BufT`) DELIBERATELY, fixing a
/// real, reproducible Tree Borrows violation found via `cargo miri
/// test` while verifying unrelated work in this same file: every real
/// call site of this function passes `buf == GLOBALS.curbuf` (see
/// `undo.rs`'s own `u_savecommon`), and this function's own body calls
/// `curbuf_is_changed()`, which independently re-derives its OWN `&mut
/// BufT` from `GLOBALS.curbuf` - a SEPARATE Tree Borrows lineage from
/// whatever reference `buf` itself might have been, even though both
/// point at the identical memory. Holding `buf` as a live `&mut BufT`
/// reference across that call invalidates it. Fixed by keeping `buf` a
/// raw pointer throughout and re-dereferencing it fresh at each field
/// access, never holding a `&mut BufT`/`&BufT` reference across the
/// `curbuf_is_changed()` call (matching `setmark_pos`'s own,
/// analogous fix for the identical class of bug).
///
/// # Safety
/// `buf` and `crate::globals::GLOBALS.curbuf` must both be valid,
/// non-null pointers to live `BufT`s (the latter touched transitively
/// via `curbuf_is_changed`).
pub unsafe fn change_warning(buf: *mut BufT, _col: i32) {
    // Note this checks the GLOBAL curbuf's changed status, NOT `buf`'s
    // own - matching the original's own `curbufIsChanged()` call
    // exactly (every real call site happens to pass `buf == curbuf`,
    // but this is not assumed/simplified away here).
    // SAFETY: forwarded from this function's own safety doc.
    let did_warn = unsafe { (*buf).b_did_warn };
    // SAFETY: forwarded from this function's own safety doc.
    let is_changed = unsafe { crate::undo::curbuf_is_changed() };
    // SAFETY: forwarded from this function's own safety doc.
    let autocmd_busy = unsafe { *crate::autocmd::AUTOCMD_BUSY.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let is_readonly = unsafe { (*buf).b_p_ro } != 0;
    if !did_warn && !is_changed && !autocmd_busy && is_readonly {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*buf).b_ro_locked += 1 };
        // SAFETY: forwarded from this function's own safety doc.
        let buf_ref = unsafe { &*buf };
        let _ = crate::autocmd::apply_autocmds(
            crate::autocmd_defs::EventT::FileChangedRO,
            None,
            None,
            false,
            Some(buf_ref),
        );
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*buf).b_ro_locked -= 1 };
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { (*buf).b_p_ro } == 0 {
            return;
        }

        // Real message display is skipped - see this function's own
        // doc comment. v:warningmsg IS set for real, matching the
        // original's set_vim_var_string(VV_WARNINGMSG, _(w_readonly), -1).
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            crate::eval::vars::set_vim_var_string(
                crate::eval::vars::VimVarIndex::Warningmsg,
                Some(b"W10: Warning: Changing a readonly file"),
            )
        };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*buf).b_did_warn = true };
        unsafe { crate::globals::GLOBALS.get_mut() }.redraw_cmdline = false;
    }
}

/// Return NUL-safe byte `i` of `s` (treats "out of bounds" the same
/// as a real NUL terminator, matching how a C string naturally reads
/// as `0` at and past its own logical end - robust regardless of
/// whether the caller's slice includes its own trailing NUL byte or
/// not). `pub(crate)` since `ops.rs`'s `skip_comment` also needs to
/// scan `b_p_com` by offset the same NUL-safe way.
pub(crate) fn byte_at(s: &[u8], i: usize) -> u8 {
    s.get(i).copied().unwrap_or(0)
}

/// Scan `line` for a comment leader, per `'comments'`
/// (`get_leader_len`). If `process` is `false` in the original this is
/// `skip_comment`'s own separate concern - this function itself always
/// "processes" (matches the original's own `get_leader_len` body,
/// which has no such flag).
///
/// @param line - line to be processed
/// @param flags - if `Some`, set to the byte offset (into
///   `crate::globals::GLOBALS.curbuf`'s `b_p_com`) where the matched
///   comment part's flags begin.
/// @param backward - true when replicating the `"O"` command's own
///   direction gate (skip parts flagged `O`, [`crate::option_vars::COM_NOBACK`]).
/// @param include_space - whether to include trailing white space
///   after the leader in the returned length.
///
/// @return the length of the leader found (`0` if none).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
pub unsafe fn get_leader_len(
    line: &[u8],
    mut flags: Option<&mut usize>,
    backward: bool,
    include_space: bool,
) -> usize {
    use crate::option_vars::{COM_BLANK, COM_END, COM_MAX_LEN, COM_MIDDLE, COM_NEST, COM_NOBACK};

    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    let com: &[u8] = curbuf.b_p_com.as_deref().unwrap_or(&[]);

    let mut got_com = false;
    // 0 doubles as "no middle match yet" AND a genuine zero-length
    // match - a real, obscure ambiguity in the original, preserved
    // literally (see this file's own module doc).
    let mut middle_match_len: usize = 0;
    let mut saved_flags: Option<usize> = None;

    let mut result: usize = 0;
    let mut i: usize = 0;
    while ascii_iswhite(i32::from(byte_at(line, i))) {
        // leading white space is ignored
        i += 1;
    }

    // Repeat to match several nested comment strings.
    while byte_at(line, i) != 0 {
        // scan through the 'comments' option for a match
        let mut found_one = false;
        // The original reuses one mutable `part_buf[COM_MAX_LEN]`
        // stack array across every part scanned in the inner loop,
        // truncating its C-string view at the colon ONLY for the part
        // that actually had one (`*string++ = NUL`). Mirrored here via
        // `part_buf`/`part_colon`, both reassigned together on every
        // inner iteration (colon-less parts leave `part_colon` as
        // `None`) - this matters because the trailing NEST check below
        // reads whichever part was scanned LAST, which is not always
        // the one that actually matched (see this file's own module
        // doc for the closely related `middle_match_len` ambiguity).
        let mut part_buf: Vec<u8> = Vec::new();
        let mut part_colon: Option<usize> = None;
        let mut list: usize = 0; // offset into com
        while byte_at(com, list) != 0 {
            // Get one option part into part_buf. Advance "list" to
            // next one.
            if !got_com
                && let Some(f) = flags.as_mut()
            {
                **f = list; // remember where flags started
            }
            let prev_list = list;
            let (buf, next_list) = copy_option_part(com, list, COM_MAX_LEN as usize, b",");
            part_buf = buf;
            list = next_list;
            let colon = match vim_strchr(&part_buf, i32::from(b':')) {
                Some(p) => p,
                None => {
                    part_colon = None;
                    continue; // missing ':', ignore this part
                }
            };
            part_colon = Some(colon);
            let mut string_start = colon + 1; // string starts right after the ':'
            let com_flags = &part_buf[..colon];

            // If we found a middle match previously, use that match
            // when this is not a middle or end.
            if middle_match_len != 0
                && vim_strchr(com_flags, i32::from(COM_MIDDLE)).is_none()
                && vim_strchr(com_flags, i32::from(COM_END)).is_none()
            {
                break;
            }

            // When we already found a nested comment, only accept
            // further nested comments.
            if got_com && vim_strchr(com_flags, i32::from(COM_NEST)).is_none() {
                continue;
            }

            // When 'O' flag present and using "O" command skip this one.
            if backward && vim_strchr(com_flags, i32::from(COM_NOBACK)).is_some() {
                continue;
            }

            // Line contents and string must match. When string starts
            // with white space, must have some white space (but the
            // amount does not need to match, there might be a mix of
            // TABs and spaces).
            if ascii_iswhite(i32::from(byte_at(&part_buf, string_start))) {
                if i == 0 || !ascii_iswhite(i32::from(byte_at(line, i - 1))) {
                    continue; // missing white space
                }
                while ascii_iswhite(i32::from(byte_at(&part_buf, string_start))) {
                    string_start += 1;
                }
            }
            let mut j = 0;
            while byte_at(&part_buf, string_start + j) != 0
                && byte_at(&part_buf, string_start + j) == byte_at(line, i + j)
            {
                j += 1;
            }
            if byte_at(&part_buf, string_start + j) != 0 {
                continue; // string doesn't match
            }
            // When 'b' flag used, there must be white space or an
            // end-of-line after the string in the line.
            if vim_strchr(com_flags, i32::from(COM_BLANK)).is_some()
                && !ascii_iswhite(i32::from(byte_at(line, i + j)))
                && byte_at(line, i + j) != 0
            {
                continue;
            }

            // We have found a match, stop searching unless this is a
            // middle comment. The middle comment can be a substring of
            // the end comment in which case it's better to return the
            // length of the end comment and its flags. Thus we keep
            // searching with middle and end matches and use an end
            // match if it matches better.
            if vim_strchr(com_flags, i32::from(COM_MIDDLE)).is_some() {
                if middle_match_len == 0 {
                    middle_match_len = j;
                    saved_flags = Some(prev_list);
                }
                continue;
            }
            if middle_match_len != 0 && j > middle_match_len {
                // Use this match instead of the middle match, since
                // it's a longer thus better match.
                middle_match_len = 0;
            }

            if middle_match_len == 0 {
                i += j;
            }
            found_one = true;
            break;
        }

        if middle_match_len != 0 {
            // Use the previously found middle match after failing to
            // find a match with an end.
            if !got_com
                && let (Some(f), Some(sf)) = (flags.as_mut(), saved_flags)
            {
                **f = sf;
            }
            i += middle_match_len;
            found_one = true;
        }

        // No match found, stop scanning.
        if !found_one {
            break;
        }

        result = i;

        // Include any trailing white space.
        while ascii_iswhite(i32::from(byte_at(line, i))) {
            i += 1;
        }

        if include_space {
            result = i;
        }

        // If this comment doesn't nest, stop here. Searches the LAST
        // part_buf scanned in the inner loop above, truncated at its
        // OWN colon if it had one - exactly matching the original's
        // `vim_strchr(part_buf, COM_NEST)` after the loop, which reads
        // whatever `part_buf` (and its NUL-truncation, if any) was
        // left holding when the inner loop exited (see this loop's own
        // `part_buf`/`part_colon` comment above).
        got_com = true;
        let last_part_flags: &[u8] = match part_colon {
            Some(c) => &part_buf[..c],
            None => &part_buf[..],
        };
        if vim_strchr(last_part_flags, i32::from(COM_NEST)).is_none() {
            break;
        }
    }
    result
}

/// NUL-safe byte at signed index `i` into `s` - `0` for any negative
/// or out-of-bounds index, matching how a C string reads as `0` both
/// before its start is never touched and past its own NUL terminator
/// (`get_last_leader_offset`'s own `i`/`off` arithmetic can transiently
/// go negative as pure loop-control values, never as a real byte
/// position).
fn byte_at_signed(s: &[u8], i: isize) -> u8 {
    if i < 0 {
        0
    } else {
        byte_at(s, i as usize)
    }
}

/// Return the offset at which the LAST comment in `line` starts,
/// scanning backward from the end (`get_last_leader_offset`). Returns
/// `None` if there is no comment in the whole line.
///
/// When `flags` is `Some`, set to the byte offset (into
/// `crate::globals::GLOBALS.curbuf`'s `b_p_com`) where the recognized
/// comment leader's flags begin - same convention as
/// [`get_leader_len`]'s own `flags` parameter.
///
/// Shares [`get_leader_len`]'s own basic per-part matching logic
/// (colon-parsing, whitespace-matching, `COM_BLANK`/`COM_MIDDLE`
/// checks) but scans backward and has NO middle-match-fallback
/// mechanism at all (a real, faithfully-preserved structural
/// difference between the two functions in the original, not an
/// oversight) - the first part that matches wins outright. After a
/// non-nesting match, a second inner pass verifies whether any OTHER
/// `'comments'` entry's own string ends with a substring that is a
/// prefix of the matched leader, adjusting how far back a FUTURE
/// (nested, `'O'` = wait, `COM_NEST`) search may need to go to avoid
/// mistaking an unrelated leader's tail for this one's start. The
/// original's own `string++` in this second pass (with no
/// null-terminator check, unlike the first pass's explicit one) is
/// translated as a graceful skip instead of blindly assuming a colon
/// exists - memory-safe and behaviorally identical for any
/// well-formed `'comments'` value (the original's own comment there -
/// "if everything is fine, this cannot actually happen" - already
/// states this is a defensive-only case).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
pub unsafe fn get_last_leader_offset(line: &[u8], mut flags: Option<&mut usize>) -> Option<usize> {
    use crate::option_vars::{COM_BLANK, COM_MAX_LEN, COM_MIDDLE, COM_NEST};

    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    let com: &[u8] = curbuf.b_p_com.as_deref().unwrap_or(&[]);

    let mut result: Option<usize> = None;
    let mut lower_check_bound: isize = 0;
    let mut i: isize = line.len() as isize;

    loop {
        i -= 1;
        if i < lower_check_bound {
            break;
        }

        let mut found_one = false;
        // Only captured once a real match is found - owned copies,
        // since the original's own `part_buf` must survive past this
        // inner loop into the LATER substring-verification pass,
        // which uses its own separate buffer (`part_buf2` there).
        let mut match_flags_offset: usize = 0;
        let mut match_com_flags: Vec<u8> = Vec::new();
        let mut com_leader: Vec<u8> = Vec::new();

        let mut list: usize = 0;
        while byte_at(com, list) != 0 {
            let flags_save = list;
            let (part_buf, next_list) = copy_option_part(com, list, COM_MAX_LEN as usize, b",");
            list = next_list;

            let colon = match vim_strchr(&part_buf, i32::from(b':')) {
                Some(p) => p,
                None => continue, // missing ':', ignore this part
            };
            let com_flags = &part_buf[..colon];
            let mut string_start = colon + 1;

            // Line contents and string must match (same whitespace
            // rule as get_leader_len).
            if ascii_iswhite(i32::from(byte_at(&part_buf, string_start))) {
                if i == 0 || !ascii_iswhite(i32::from(byte_at_signed(line, i - 1))) {
                    continue;
                }
                while ascii_iswhite(i32::from(byte_at(&part_buf, string_start))) {
                    string_start += 1;
                }
            }
            let mut j: isize = 0;
            while byte_at(&part_buf, string_start + j as usize) != 0
                && byte_at(&part_buf, string_start + j as usize) == byte_at_signed(line, i + j)
            {
                j += 1;
            }
            if byte_at(&part_buf, string_start + j as usize) != 0 {
                continue; // string doesn't match
            }

            // When 'b' flag used, there must be white space or an
            // end-of-line after the string in the line.
            if vim_strchr(com_flags, i32::from(COM_BLANK)).is_some()
                && !ascii_iswhite(i32::from(byte_at_signed(line, i + j)))
                && byte_at_signed(line, i + j) != 0
            {
                continue;
            }

            if vim_strchr(com_flags, i32::from(COM_MIDDLE)).is_some() {
                // For a middlepart comment, only consider it to match
                // if everything before the current position in the
                // line is whitespace.
                let mut jj: isize = 0;
                while jj <= i && ascii_iswhite(i32::from(byte_at_signed(line, jj))) {
                    jj += 1;
                }
                if jj < i {
                    continue;
                }
            }

            // We have found a match, stop searching (no middle-match
            // fallback in this function, unlike get_leader_len).
            found_one = true;
            match_flags_offset = flags_save;
            match_com_flags = com_flags.to_vec();
            com_leader = part_buf[string_start..].to_vec();
            break;
        }

        if found_one {
            if let Some(f) = flags.as_mut() {
                **f = match_flags_offset;
            }

            result = Some(i as usize);

            // If this comment nests, continue searching (further
            // back, toward the start of the line).
            if vim_strchr(&match_com_flags, i32::from(COM_NEST)).is_some() {
                continue;
            }

            lower_check_bound = i;

            // Let's verify whether the comment leader found is a
            // substring of other comment leaders. If it is, adjust
            // lower_check_bound so a future search doesn't mistake an
            // unrelated leader's own tail for this one's start.
            let mut cl_start = 0;
            while ascii_iswhite(i32::from(byte_at(&com_leader, cl_start))) {
                cl_start += 1;
            }
            let com_leader = &com_leader[cl_start..];
            let len1 = com_leader.len() as isize;

            let mut list2: usize = 0;
            while byte_at(com, list2) != 0 {
                let flags_save2 = list2;
                let (part_buf2, next_list2) = copy_option_part(com, list2, COM_MAX_LEN as usize, b",");
                list2 = next_list2;

                if flags_save2 == match_flags_offset {
                    continue;
                }
                let colon2 = match vim_strchr(&part_buf2, i32::from(b':')) {
                    Some(p) => p,
                    // The original does NOT check this (a bare
                    // `string++` with no null check) - its own comment
                    // on the FIRST loop's equivalent check already
                    // states this "cannot actually happen" for a
                    // well-formed 'comments' value; skipping
                    // gracefully here is memory-safe and behaviorally
                    // identical for any real input.
                    None => continue,
                };
                let mut string2_start = colon2 + 1;
                while ascii_iswhite(i32::from(byte_at(&part_buf2, string2_start))) {
                    string2_start += 1;
                }
                let string2 = &part_buf2[string2_start..];
                let len2 = string2.len() as isize;
                if len2 == 0 {
                    continue;
                }

                // Now verify whether string2 ends with a substring
                // beginning com_leader.
                let mut off: isize = if len2 > i { i } else { len2 };
                while off > 0 && off + len1 > len2 {
                    off -= 1;
                    let cmplen = (len2 - off) as usize; // == string2.len() - off, always
                    // cmplen > com_leader.len() can never match (the
                    // original's own strncmp would compare
                    // com_leader's NUL terminator against a real,
                    // non-NUL byte of string2 at that position) -
                    // skip rather than panic on a too-long slice.
                    if cmplen <= com_leader.len()
                        && string2[off as usize..] == com_leader[..cmplen]
                    {
                        lower_check_bound = lower_check_bound.min(i - off);
                    }
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::buf_get_changedtick;
    use crate::buffer_defs::WlineT;
    use crate::pos_defs::PosT;

    /// A window with a small, fully-valid `w_lines[]` display cache
    /// covering lines 1..=4, so the invalidation pass has something
    /// real to walk.
    fn win_with_line_cache(buf: *mut BufT, cursor_lnum: LinenrT) -> WinT {
        let lines: Vec<WlineT> = (1..=4)
            .map(|n| WlineT {
                wl_lnum: n,
                wl_lastlnum: n,
                wl_foldend: n,
                wl_valid: true,
                ..Default::default()
            })
            .collect();
        WinT {
            w_buffer: buf,
            w_cursor: PosT { lnum: cursor_lnum, col: 0, coladd: 0 },
            w_topline: 1,
            w_botline: 5,
            w_lines_valid: 4,
            w_lines: lines,
            ..Default::default()
        }
    }

    /// RAII guard installing a window/buffer/tabpage chain for the
    /// `changed_bytes`/`changed_common` tests, restoring every global
    /// they touch on drop (even on panic). Self-locking, matching this
    /// crate's established per-file test-guard convention.
    struct ChangedGuard {
        prev_curwin: *mut WinT,
        prev_curbuf: *mut BufT,
        prev_firstwin: *mut WinT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
        prev_state: i32,
        prev_must_redraw: i32,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl ChangedGuard {
        fn set(
            win: *mut WinT,
            buf: *mut BufT,
            tab: *mut crate::buffer_defs::TabpageT,
        ) -> Self {
            let _lock = crate::globals::global_state_test_lock();
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = ChangedGuard {
                prev_curwin: g.curwin,
                prev_curbuf: g.curbuf,
                prev_firstwin: g.firstwin,
                prev_curtab: g.curtab,
                prev_state: g.State,
                prev_must_redraw: g.must_redraw,
                _lock,
            };
            g.curwin = win;
            g.curbuf = buf;
            g.firstwin = win;
            g.curtab = tab;
            g.State = crate::state_defs::mode::NORMAL as i32;
            g.must_redraw = 0;
            g.cmdmod.cmod_flags = 0;
            guard
        }
    }

    impl Drop for ChangedGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            g.curwin = self.prev_curwin;
            g.curbuf = self.prev_curbuf;
            g.firstwin = self.prev_firstwin;
            g.curtab = self.prev_curtab;
            g.State = self.prev_state;
            g.must_redraw = self.prev_must_redraw;
        }
    }

    /// Builds a boxed buffer/window pair. The caller derives the raw
    /// pointers AFTER this returns, via [`fixture_ptrs`], because
    /// moving the `Box`es out of here invalidates any pointer derived
    /// inside - Tree Borrows tracks the borrow lineage, not just the
    /// address.
    fn changed_fixture() -> (Box<BufT>, Box<WinT>) {
        let buf = Box::new(BufT {
            b_ml: crate::memline_defs::MemlineT {
                ml_line_count: 10,
                ..Default::default()
            },
            ..Default::default()
        });
        let win = Box::new(win_with_line_cache(std::ptr::null_mut(), 1));
        (buf, win)
    }

    /// Wires the pair together through ONE derived pointer each, so
    /// the reference a callee reborrows through `GLOBALS` shares a
    /// single provenance with the one stored in `w_buffer`.
    fn fixture_ptrs(buf: &mut BufT, win: &mut WinT) -> (*mut BufT, *mut WinT) {
        let buf_ptr: *mut BufT = buf;
        win.w_buffer = buf_ptr;
        let win_ptr: *mut WinT = win;
        (buf_ptr, win_ptr)
    }

    /// Builds a buffer with a REAL memline holding one line, plus a
    /// window whose cursor sits at `col` (0-based). `del_bytes` reads
    /// and writes the line through the memline, so unlike the
    /// `changed_fixture` above this needs genuine storage.
    ///
    /// Returns the boxes so the caller keeps them alive; pointers are
    /// derived afterwards via [`fixture_ptrs`].
    fn del_fixture(line: &[u8], col: ColnrT) -> (Box<BufT>, Box<WinT>) {
        let mut buf = Box::new(BufT::default());
        assert_eq!(
            unsafe { crate::memline::ml_open(&mut buf) },
            crate::vim_defs::OK
        );
        let mut owned = line.to_vec();
        owned.push(0);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, &owned) },
            crate::vim_defs::OK
        );
        // A real undo header, so u_force_get_undo_header hands one
        // back rather than trying to create one (which needs undo
        // state this fixture doesn't build).
        buf.b_u_curhead = Box::into_raw(Box::new(crate::undo_defs::UHeader::default()));
        let mut win = Box::new(WinT {
            w_cursor: PosT { lnum: 1, col, coladd: 0 },
            w_topline: 1,
            w_botline: 2,
            ..Default::default()
        });
        win.w_lines_valid = 0;
        (buf, win)
    }

    fn close_del_fixture(mut buf: Box<BufT>) {
        unsafe {
            if !buf.b_u_curhead.is_null() {
                drop(Box::from_raw(buf.b_u_curhead));
                buf.b_u_curhead = std::ptr::null_mut();
            }
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    /// Like [`del_fixture`], but builds a buffer with several lines,
    /// which `del_lines` needs in order to have anything to remove.
    fn del_fixture_lines(lines: &[&[u8]], lnum: LinenrT) -> (Box<BufT>, Box<WinT>) {
        let mut buf = Box::new(BufT::default());
        assert_eq!(
            unsafe { crate::memline::ml_open(&mut buf) },
            crate::vim_defs::OK
        );
        for (i, line) in lines.iter().enumerate() {
            let mut owned = line.to_vec();
            owned.push(0);
            if i == 0 {
                assert_eq!(
                    unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, &owned) },
                    crate::vim_defs::OK
                );
            } else {
                assert_eq!(
                    unsafe {
                        crate::memline::ml_append_buf(
                            &mut buf,
                            i as LinenrT,
                            &owned,
                            owned.len() as i32,
                            false,
                        )
                    },
                    crate::vim_defs::OK
                );
            }
        }
        buf.b_u_curhead = Box::into_raw(Box::new(crate::undo_defs::UHeader::default()));
        let win = Box::new(WinT {
            w_cursor: PosT { lnum, col: 0, coladd: 0 },
            w_topline: 1,
            w_botline: lines.len() as LinenrT + 1,
            ..Default::default()
        });
        (buf, win)
    }

    #[test]
    fn del_lines_removes_the_requested_count() {
        // Cross-verified against real nvim: lines a..e with the cursor
        // on line 2 and "2dd" leaves "a,d,e" with the cursor on line 2.
        let (mut buf, mut win) = del_fixture_lines(&[b"a", b"b", b"c", b"d", b"e"], 2);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { del_lines(2, false) };

        assert_eq!(unsafe { (*buf_ptr).b_ml.ml_line_count }, 3);
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"a\0");
        assert_eq!(unsafe { crate::memline::ml_get(2) }, b"d\0");
        assert_eq!(unsafe { crate::memline::ml_get(3) }, b"e\0");
        assert_eq!(unsafe { (*win_ptr).w_cursor.lnum }, 2);
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 0);

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn del_lines_stops_when_the_buffer_becomes_empty() {
        // Cross-verified against real nvim: a 2-line buffer with "9dd"
        // ends up with one empty line, not zero lines.
        let (mut buf, mut win) = del_fixture_lines(&[b"a", b"b"], 1);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { del_lines(9, false) };

        assert_eq!(unsafe { (*buf_ptr).b_ml.ml_line_count }, 1);
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"\0");
        assert_eq!(unsafe { (*win_ptr).w_cursor.lnum }, 1);

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn del_lines_with_a_non_positive_count_is_a_noop() {
        let (mut buf, mut win) = del_fixture_lines(&[b"a", b"b"], 1);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { del_lines(0, false) };

        assert_eq!(unsafe { (*buf_ptr).b_ml.ml_line_count }, 2);
        assert_eq!(unsafe { (*buf_ptr).b_changed }, 0);

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn ins_bytes_len_inserts_each_character_whole() {
        // Cross-verified against real nvim: "af" with the cursor on
        // column 2 (1-based) and inserting "bécd" yields "abécdf" with
        // strlen 7, so the 2-byte é is handed to ins_char_bytes as one
        // character rather than split across two calls.
        let (mut buf, mut win) = del_fixture(b"af", 1);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { ins_bytes_len("bécd".as_bytes()) };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, "abécdf\0".as_bytes());
        assert_eq!(unsafe { crate::memline::ml_get_len(1) }, 7);
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 6, "advanced 5 bytes");

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn ins_bytes_stops_at_a_trailing_nul() {
        // The original takes a NUL-terminated char * and measures it
        // with strlen, so the NUL itself must never be inserted.
        let (mut buf, mut win) = del_fixture(b"ad", 1);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { ins_bytes(b"bc\0") };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"abcd\0");
        assert_eq!(unsafe { crate::memline::ml_get_len(1) }, 4);

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn ins_bytes_len_with_an_empty_slice_is_a_noop() {
        let (mut buf, mut win) = del_fixture(b"ab", 1);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { ins_bytes_len(b"") };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"ab\0");
        assert_eq!(unsafe { (*buf_ptr).b_changed }, 0, "nothing changed");

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn ins_char_inserts_a_single_byte_character() {
        // Cross-verified against real nvim: "abd" with the cursor on
        // column 3 (1-based) and inserting 'c' yields "abcd".
        let (mut buf, mut win) = del_fixture(b"abd", 2);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { ins_char(i32::from(b'c')) };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"abcd\0");
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 3, "advanced by one");
        assert_ne!(unsafe { (*buf_ptr).b_changed }, 0);

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn ins_char_inserts_a_whole_multibyte_character() {
        // Cross-verified against real nvim: "ab" with the cursor on
        // column 1 and inserting 'é' yields "éab" with strlen 4, so
        // the 2-byte encoding is stored whole and the cursor advances
        // by both bytes.
        let (mut buf, mut win) = del_fixture(b"ab", 0);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { ins_char(0xE9) };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, "éab\0".as_bytes());
        assert_eq!(unsafe { crate::memline::ml_get_len(1) }, 4);
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 2, "advanced by two");

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn ins_char_bytes_appends_at_the_end_of_the_line() {
        let (mut buf, mut win) = del_fixture(b"ab", 2);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { ins_char_bytes(b"c") };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"abc\0");
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 3);

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn ins_char_with_revins_set_leaves_the_cursor_put() {
        // 'revins' suppresses the cursor advance outside Replace mode
        // - the real `!p_ri || (State & REPLACE_FLAG)` condition.
        let (mut buf, mut win) = del_fixture(b"ab", 1);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);
        let prev_ri = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ri;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ri = 1;

        unsafe { ins_char(i32::from(b'X')) };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"aXb\0");
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 1, "cursor not advanced");

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ri = prev_ri;
        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn ins_str_inserts_at_the_cursor_and_advances_it() {
        // Cross-verified against real nvim: "abef" with the cursor on
        // column 3 (1-based) and inserting "cd" yields "abcdef".
        // ins_str advances the cursor past what it inserted, so the
        // 0-based column goes 2 -> 4 (nvim then reports column 4
        // 1-based once <Esc> steps back one).
        let (mut buf, mut win) = del_fixture(b"abef", 2);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { ins_str(b"cd") };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"abcdef\0");
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 4);
        assert_ne!(unsafe { (*buf_ptr).b_changed }, 0);

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn ins_str_at_column_zero_prepends() {
        let (mut buf, mut win) = del_fixture(b"cd", 0);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { ins_str(b"ab") };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"abcd\0");
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 2);

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn ins_str_at_the_end_appends() {
        let (mut buf, mut win) = del_fixture(b"ab", 2);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { ins_str(b"cd") };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"abcd\0");
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 4);

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn truncate_line_cuts_from_the_cursor_to_the_end() {
        // Cross-verified against real nvim: "abcdef" with the cursor
        // on column 3 (1-based) and "D" yields "ab" with the cursor
        // pulled back to column 2.
        let (mut buf, mut win) = del_fixture(b"abcdef", 2);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { truncate_line(true) };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"ab\0");
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 1, "fixpos stepped back");
        assert_ne!(unsafe { (*buf_ptr).b_changed }, 0);

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn truncate_line_at_column_zero_empties_the_line() {
        // Cross-verified against real nvim: "abcdef" with the cursor
        // on column 1 and "D" empties the line, leaving the cursor on
        // column 1 - fixpos cannot step back past zero.
        let (mut buf, mut win) = del_fixture(b"abcdef", 0);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { truncate_line(true) };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"\0");
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 0);

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn truncate_line_without_fixpos_leaves_the_cursor_alone() {
        let (mut buf, mut win) = del_fixture(b"abcdef", 2);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { truncate_line(false) };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"ab\0");
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 2, "left on the NUL");

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn del_char_removes_one_whole_multibyte_character() {
        // Cross-verified against real nvim: "aébc" with the cursor on
        // column 2 (1-based, the 2-byte é) and "x" yields "abc" with
        // the cursor still on column 2.
        let (mut buf, mut win) = del_fixture("aébc".as_bytes(), 1);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        assert_eq!(unsafe { del_char(true) }, crate::vim_defs::OK);

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"abc\0");
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 1);

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn del_chars_counts_characters_not_bytes() {
        // Cross-verified against real nvim: "aébcd" with the cursor on
        // column 2 and "3x" deletes é, b and c, yielding "ad" with the
        // cursor still on column 2.
        let (mut buf, mut win) = del_fixture("aébcd".as_bytes(), 1);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        assert_eq!(unsafe { del_chars(3, true) }, crate::vim_defs::OK);

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"ad\0");
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 1);

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn del_char_fails_on_the_nul_past_the_end() {
        let (mut buf, mut win) = del_fixture(b"ab", 2);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        assert_eq!(unsafe { del_char(true) }, crate::vim_defs::FAIL);
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"ab\0");

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn del_chars_stops_at_the_end_of_the_line() {
        // Asking for more characters than remain deletes only what is
        // there - the loop's own `*p != NUL` guard.
        let (mut buf, mut win) = del_fixture(b"abc", 1);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        assert_eq!(unsafe { del_chars(99, true) }, crate::vim_defs::OK);

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"a\0");

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn del_bytes_removes_the_requested_range() {
        // Cross-verified against real nvim: "abcdef" with the cursor
        // on column 3 (1-based) and "2x" yields "abef" with the cursor
        // still on column 3.
        let (mut buf, mut win) = del_fixture(b"abcdef", 2);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        assert_eq!(unsafe { del_bytes(2, true, false) }, crate::vim_defs::OK);

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"abef\0");
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 2, "cursor unmoved");
        assert_ne!(unsafe { (*buf_ptr).b_changed }, 0);

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn del_bytes_fixpos_steps_back_off_the_trailing_nul() {
        // Cross-verified against real nvim: "abcdef" with the cursor
        // on column 6 (1-based, the last char) and "x" yields "abcde"
        // with the cursor pulled back to column 5.
        let (mut buf, mut win) = del_fixture(b"abcdef", 5);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        assert_eq!(unsafe { del_bytes(1, true, false) }, crate::vim_defs::OK);

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"abcde\0");
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 4, "pulled back one");

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn del_bytes_clamps_a_count_past_the_end_of_the_line() {
        // Cross-verified against real nvim: "ab" with the cursor on
        // column 1 and "5x" empties the line, leaving the cursor on
        // column 1.
        let (mut buf, mut win) = del_fixture(b"ab", 0);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        assert_eq!(unsafe { del_bytes(5, true, false) }, crate::vim_defs::OK);

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"\0");
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 0);

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn del_bytes_fails_on_the_nul_past_the_end() {
        let (mut buf, mut win) = del_fixture(b"ab", 2);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        assert_eq!(unsafe { del_bytes(1, true, false) }, crate::vim_defs::FAIL);
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"ab\0");

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn del_bytes_zero_count_succeeds_without_changing_anything() {
        let (mut buf, mut win) = del_fixture(b"ab", 0);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        assert_eq!(unsafe { del_bytes(0, true, false) }, crate::vim_defs::OK);
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"ab\0");
        assert_eq!(unsafe { (*buf_ptr).b_changed }, 0, "no change recorded");

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn del_bytes_rejects_a_negative_count() {
        let (mut buf, mut win) = del_fixture(b"ab", 0);
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        assert_eq!(unsafe { del_bytes(-1, true, false) }, crate::vim_defs::FAIL);
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"ab\0");

        drop(_guard);
        close_del_fixture(buf);
    }

    #[test]
    fn inserted_bytes_records_an_extmark_splice_and_marks_the_change() {
        let (mut buf, mut win) = changed_fixture();
        // A real undo header, so u_force_get_undo_header hands one
        // back instead of trying to create one (which would need a
        // live memline this fixture deliberately doesn't build).
        let uhp = Box::into_raw(Box::new(crate::undo_defs::UHeader::default()));
        buf.b_u_curhead = uhp;
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);
        let prev_pending = *unsafe { crate::extmark::CURBUF_SPLICE_PENDING.get_mut() };
        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = 0 };

        unsafe { inserted_bytes(3, 2, 0, 4) };

        assert_ne!(unsafe { (*buf_ptr).b_changed }, 0);
        assert_eq!(unsafe { (*buf_ptr).b_mod_top }, 3);
        assert_eq!(unsafe { (*buf_ptr).b_last_change.mark.col }, 2);
        // The splice really happened: it was recorded for undo.
        assert_eq!(unsafe { &(*uhp).uh_extmark }.len(), 1);

        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = prev_pending };
        unsafe { (*buf_ptr).b_u_curhead = std::ptr::null_mut() };
        drop(unsafe { Box::from_raw(uhp) });
    }

    #[test]
    fn inserted_bytes_skips_the_splice_when_one_is_already_pending() {
        let (mut buf, mut win) = changed_fixture();
        let uhp = Box::into_raw(Box::new(crate::undo_defs::UHeader::default()));
        buf.b_u_curhead = uhp;
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);
        let prev_pending = *unsafe { crate::extmark::CURBUF_SPLICE_PENDING.get_mut() };
        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = 1 };

        // The caller will do its own splice, so only the
        // changed_bytes half runs.
        unsafe { inserted_bytes(3, 2, 0, 4) };

        assert_ne!(unsafe { (*buf_ptr).b_changed }, 0);
        assert_eq!(unsafe { (*buf_ptr).b_mod_top }, 3);
        assert!(
            unsafe { &(*uhp).uh_extmark }.is_empty(),
            "no splice recorded when one is already pending"
        );

        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = prev_pending };
        unsafe { (*buf_ptr).b_u_curhead = std::ptr::null_mut() };
        drop(unsafe { Box::from_raw(uhp) });
    }

    #[test]
    fn changed_lines_records_the_region_and_marks_the_buffer() {
        let (mut buf, mut win) = changed_fixture();
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { changed_lines(buf_ptr, 2, 0, 5, 0, true) };

        assert_ne!(unsafe { (*buf_ptr).b_changed }, 0);
        assert_eq!(unsafe { (*buf_ptr).b_mod_top }, 2);
        assert_eq!(unsafe { (*buf_ptr).b_mod_bot }, 5);
        assert_eq!(unsafe { (*buf_ptr).b_mod_xlines }, 0);
    }

    #[test]
    fn appended_lines_buf_records_lines_added_below_lnum() {
        let (mut buf, mut win) = changed_fixture();
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        // 3 lines appended below line 4.
        unsafe { appended_lines_buf(buf_ptr, 4, 3) };

        assert_eq!(unsafe { (*buf_ptr).b_mod_top }, 5);
        assert_eq!(unsafe { (*buf_ptr).b_mod_bot }, 8);
        assert_eq!(unsafe { (*buf_ptr).b_mod_xlines }, 3);
    }

    #[test]
    fn appended_lines_uses_the_current_buffer() {
        let (mut buf, mut win) = changed_fixture();
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { appended_lines(4, 3) };

        assert_eq!(unsafe { (*buf_ptr).b_mod_top }, 5);
        assert_eq!(unsafe { (*buf_ptr).b_mod_xlines }, 3);
    }

    #[test]
    fn deleted_lines_buf_records_a_negative_xtra() {
        let (mut buf, mut win) = changed_fixture();
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        // 2 lines deleted at line 4.
        unsafe { deleted_lines_buf(buf_ptr, 4, 2) };

        assert_eq!(unsafe { (*buf_ptr).b_mod_top }, 4);
        assert_eq!(unsafe { (*buf_ptr).b_mod_bot }, 4, "lnume(6) + xtra(-2)");
        assert_eq!(unsafe { (*buf_ptr).b_mod_xlines }, -2);
    }

    #[test]
    fn deleted_lines_uses_the_current_buffer() {
        let (mut buf, mut win) = changed_fixture();
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { deleted_lines(4, 2) };

        assert_eq!(unsafe { (*buf_ptr).b_mod_top }, 4);
        assert_eq!(unsafe { (*buf_ptr).b_mod_xlines }, -2);
    }

    #[test]
    fn deleted_lines_mark_adjusts_extmarks_then_records_the_change() {
        let (mut buf, mut win) = changed_fixture();
        let uhp = Box::into_raw(Box::new(crate::undo_defs::UHeader::default()));
        buf.b_u_curhead = uhp;
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);
        let prev_pending = *unsafe { crate::extmark::CURBUF_SPLICE_PENDING.get_mut() };
        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = 0 };

        // 2 lines deleted at line 4.
        unsafe { deleted_lines_mark(4, 2) };

        assert_eq!(unsafe { (*buf_ptr).b_mod_top }, 4);
        assert_eq!(unsafe { (*buf_ptr).b_mod_xlines }, -2);
        assert_ne!(unsafe { (*buf_ptr).b_changed }, 0);
        // The deletion was recorded for undo by extmark_adjust.
        assert_eq!(unsafe { &(*uhp).uh_extmark }.len(), 1);

        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = prev_pending };
        unsafe { (*buf_ptr).b_u_curhead = std::ptr::null_mut() };
        drop(unsafe { Box::from_raw(uhp) });
    }

    #[test]
    fn changed_bytes_marks_the_buffer_and_schedules_a_redraw() {
        let (mut buf, mut win) = changed_fixture();
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        assert_eq!(unsafe { (*buf_ptr).b_changed }, 0);
        unsafe { changed_bytes(3, 2) };

        assert_ne!(unsafe { (*buf_ptr).b_changed }, 0, "buffer marked modified");
        assert!(unsafe { (*buf_ptr).b_mod_set }, "a redraw region was recorded");
        assert_eq!(unsafe { (*buf_ptr).b_mod_top }, 3);
        assert_eq!(unsafe { (*buf_ptr).b_mod_bot }, 4);
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.must_redraw,
            crate::drawscreen::UPD_VALID
        );
    }

    #[test]
    fn changed_bytes_sets_the_last_change_mark_and_changelist() {
        let (mut buf, mut win) = changed_fixture();
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { changed_bytes(3, 7) };

        assert_eq!(unsafe { (*buf_ptr).b_last_change.mark.lnum }, 3);
        assert_eq!(unsafe { (*buf_ptr).b_last_change.mark.col }, 7);
        assert_eq!(
            unsafe { (*buf_ptr).b_changelistlen },
            1,
            "first change starts the list"
        );
        assert_eq!(unsafe { (*buf_ptr).b_changelist[0].mark.lnum }, 3);
        assert_eq!(
            unsafe { (*win_ptr).w_changelistidx },
            1,
            "current window sits after the last change"
        );
    }

    #[test]
    fn changed_bytes_respects_keepjumps() {
        let (mut buf, mut win) = changed_fixture();
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);
        unsafe { crate::globals::GLOBALS.get_mut() }.cmdmod.cmod_flags =
            crate::ex_cmds_defs::cmod::KEEPJUMPS;

        unsafe { changed_bytes(3, 2) };

        assert_eq!(
            unsafe { (*buf_ptr).b_last_change.mark.lnum },
            0,
            "'. mark left alone"
        );
        assert_eq!(
            unsafe { (*buf_ptr).b_changelistlen },
            0,
            "changelist left alone"
        );
        assert_ne!(
            unsafe { (*buf_ptr).b_changed },
            0,
            "but the buffer is still modified"
        );
    }

    #[test]
    fn changed_bytes_does_not_add_a_second_nearby_changelist_entry() {
        let (mut buf, mut win) = changed_fixture();
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { changed_bytes(3, 2) };
        assert_eq!(unsafe { (*buf_ptr).b_changelistlen }, 1);

        // A new undo-able change on the SAME line, a short distance
        // away, overwrites the entry rather than adding one - this is
        // what stops typing "xxxxx" from filling the changelist.
        unsafe { (*buf_ptr).b_new_change = true };
        unsafe { changed_bytes(3, 4) };

        assert_eq!(unsafe { (*buf_ptr).b_changelistlen }, 1);
        assert_eq!(unsafe { (*buf_ptr).b_changelist[0].mark.col }, 4);
    }

    #[test]
    fn changed_bytes_adds_an_entry_for_a_change_on_another_line() {
        let (mut buf, mut win) = changed_fixture();
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { changed_bytes(3, 2) };
        unsafe { (*buf_ptr).b_new_change = true };
        unsafe { changed_bytes(8, 0) };

        assert_eq!(unsafe { (*buf_ptr).b_changelistlen }, 2);
        assert_eq!(unsafe { (*buf_ptr).b_changelist[0].mark.lnum }, 3);
        assert_eq!(unsafe { (*buf_ptr).b_changelist[1].mark.lnum }, 8);
        assert_eq!(unsafe { (*win_ptr).w_changelistidx }, 2);
    }

    #[test]
    fn changed_bytes_invalidates_the_line_cache() {
        let (mut buf, mut win) = changed_fixture();
        let (buf_ptr, win_ptr) = fixture_ptrs(&mut buf, &mut win);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = ChangedGuard::set(win_ptr, buf_ptr, &mut tab);

        unsafe { changed_bytes(3, 0) };

        assert!(
            !unsafe { &(*win_ptr).w_lines }[2].wl_valid,
            "changed line invalidated"
        );
        assert!(
            unsafe { &(*win_ptr).w_lines }[0].wl_valid,
            "line above untouched"
        );
        assert_eq!(
            unsafe { (*win_ptr).w_redr_type },
            crate::drawscreen::UPD_VALID
        );
    }

    #[test]
    fn invalidate_win_marks_covered_lines_invalid() {
        let mut buf = BufT::default();
        let mut win = win_with_line_cache(&mut buf, 1);

        // Change covering lines 2..3 with no line-count change.
        unsafe { changed_lines_invalidate_win(&mut win, 2, 0, 4, 0) };

        assert!(!win.w_lines[1].wl_valid, "line 2 is inside the change");
        assert!(!win.w_lines[2].wl_valid, "line 3 is inside the change");
        assert!(win.w_lines[3].wl_valid, "line 4 is below it and unshifted");
        assert_eq!(win.w_lines[3].wl_lnum, 4);
    }

    #[test]
    fn invalidate_win_shifts_entries_below_the_change() {
        let mut buf = BufT::default();
        let mut win = win_with_line_cache(&mut buf, 1);

        // Two lines inserted at line 2: entries below shift down.
        unsafe { changed_lines_invalidate_win(&mut win, 2, 0, 2, 2) };

        assert_eq!(win.w_lines[1].wl_lnum, 4);
        assert_eq!(win.w_lines[1].wl_lastlnum, 4);
        assert_eq!(win.w_lines[1].wl_foldend, 4);
        assert!(win.w_lines[1].wl_valid, "still valid, just renumbered");
        assert_eq!(win.w_lines[3].wl_lnum, 6);
    }

    #[test]
    fn invalidate_win_never_renumbers_entry_zero() {
        let mut buf = BufT::default();
        let mut win = win_with_line_cache(&mut buf, 1);

        // The change starts at line 1, so entry 0 would otherwise be
        // shifted; it is invalidated instead, since it is what
        // w_topline is compared against.
        unsafe { changed_lines_invalidate_win(&mut win, 1, 0, 1, 3) };

        assert!(!win.w_lines[0].wl_valid);
        assert_eq!(win.w_lines[0].wl_lnum, 1, "left untouched, not shifted");
    }

    #[test]
    fn invalidate_win_invalidates_a_folded_range_containing_the_change() {
        let mut buf = BufT::default();
        let mut win = win_with_line_cache(&mut buf, 1);
        // Entry 0 stands for a fold covering lines 1..=9.
        win.w_lines[0].wl_lastlnum = 9;

        // The change is at line 6, inside that folded range, even
        // though the entry's own wl_lnum is below lnum.
        unsafe { changed_lines_invalidate_win(&mut win, 6, 0, 7, 0) };

        assert!(!win.w_lines[0].wl_valid);
    }

    #[test]
    fn invalidate_buf_only_touches_windows_on_that_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf_a = BufT::default();
        let mut buf_b = BufT::default();
        let mut win_b = win_with_line_cache(&mut buf_b, 1);
        let mut win_a = win_with_line_cache(&mut buf_a, 1);
        win_a.w_next = &mut win_b;

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = g.firstwin;
        g.firstwin = &mut win_a;

        unsafe { changed_lines_invalidate_buf(&mut buf_a, 2, 0, 4, 0) };

        assert!(!win_a.w_lines[1].wl_valid);
        assert!(win_b.w_lines[1].wl_valid, "other buffer's window untouched");

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
    }

    #[test]
    fn changed_lines_redraw_buf_sets_a_fresh_region() {
        let mut buf = BufT::default();
        assert!(!buf.b_mod_set);

        unsafe { changed_lines_redraw_buf(&mut buf, 5, 8, 0) };

        assert!(buf.b_mod_set);
        assert_eq!(buf.b_mod_top, 5);
        assert_eq!(buf.b_mod_bot, 8);
        assert_eq!(buf.b_mod_xlines, 0);
    }

    #[test]
    fn changed_lines_redraw_buf_adds_xtra_to_a_fresh_region() {
        let mut buf = BufT::default();

        // Three lines inserted at line 4.
        unsafe { changed_lines_redraw_buf(&mut buf, 4, 4, 3) };

        assert_eq!(buf.b_mod_top, 4);
        assert_eq!(buf.b_mod_bot, 7, "bottom accounts for the added lines");
        assert_eq!(buf.b_mod_xlines, 3);
    }

    #[test]
    fn changed_lines_redraw_buf_widens_an_existing_region() {
        let mut buf = BufT::default();
        unsafe { changed_lines_redraw_buf(&mut buf, 10, 12, 0) };

        // A second change above the first extends the top, and the
        // bottom is kept at the widest point seen.
        unsafe { changed_lines_redraw_buf(&mut buf, 3, 4, 0) };

        assert_eq!(buf.b_mod_top, 3);
        assert_eq!(buf.b_mod_bot, 12);
        assert_eq!(buf.b_mod_xlines, 0);
    }

    #[test]
    fn changed_lines_redraw_buf_shifts_the_old_bottom_by_xtra() {
        let mut buf = BufT::default();
        unsafe { changed_lines_redraw_buf(&mut buf, 10, 20, 0) };

        // A change at line 2 that deletes 3 lines: the pending bottom
        // was below it, so it slides up by xtra, and xlines accumulates.
        unsafe { changed_lines_redraw_buf(&mut buf, 2, 5, -3) };

        assert_eq!(buf.b_mod_top, 2);
        assert_eq!(buf.b_mod_bot, 17, "old bottom 20 shifted by -3");
        assert_eq!(buf.b_mod_xlines, -3);
    }

    #[test]
    fn changed_lines_redraw_buf_clamps_a_shifted_bottom_to_lnum() {
        let mut buf = BufT::default();
        unsafe { changed_lines_redraw_buf(&mut buf, 4, 5, 0) };

        // Deleting far more lines than the region spans would push the
        // adjusted bottom above lnum; it is clamped to lnum instead.
        unsafe { changed_lines_redraw_buf(&mut buf, 4, 4, -50) };

        assert_eq!(buf.b_mod_top, 4);
        assert_eq!(buf.b_mod_bot, 4);
        assert_eq!(buf.b_mod_xlines, -50);
    }

    #[test]
    fn changed_lines_redraw_buf_leaves_lnume_alone_without_marks() {
        let mut buf = BufT::default();
        assert_eq!(buf.b_marktree.n_keys, 0);

        // xtra != 0, but an empty marktree means no decoration
        // adjustment, so lnume is used as given.
        unsafe { changed_lines_redraw_buf(&mut buf, 1, 3, 2) };

        assert_eq!(buf.b_mod_bot, 5, "lnume(3) + xtra(2), not widened");
    }

    #[test]
    fn file_ff_differs_false_for_never_loaded_buffer() {
        let mut buf = BufT { b_flags: b_flags::BF_NEVERLOADED as i32, ..Default::default() };
        assert!(!unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn file_ff_differs_false_for_new_empty_buffer_when_ignoring_empty() {
        // ml_open touches shared GLOBALS.got_int internally via
        // mf_sync - must hold the lock like every other GlobalCell-
        // touching test (see memfile.rs's mf_sync tests for the same
        // reasoning).
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        buf.b_flags = b_flags::BF_NEW as i32;

        assert!(!unsafe { file_ff_differs(&mut buf, true) });

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn file_ff_differs_true_when_new_empty_buffer_not_ignored() {
        // See file_ff_differs_false_for_new_empty_buffer_when_ignoring_
        // empty's own comment: ml_open touches shared GLOBALS.got_int.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        buf.b_flags = b_flags::BF_NEW as i32;
        // b_start_ffc defaults to 0, which differs from b_p_ff's
        // (also-defaulted) empty/None first byte only if we force a
        // mismatch - set b_p_ff so the ffc check itself trips.
        buf.b_p_ff = Some(b"unix".to_vec());
        buf.b_start_ffc = i32::from(b'd'); // "dos", deliberately different

        assert!(unsafe { file_ff_differs(&mut buf, false) });

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn file_ff_differs_true_when_fileformat_first_char_changed() {
        let mut buf = BufT {
            b_p_ff: Some(b"dos".to_vec()),
            b_start_ffc: i32::from(b'u'), // was "unix" when editing started
            ..Default::default()
        };
        assert!(unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn file_ff_differs_false_when_nothing_changed() {
        let mut buf = BufT {
            b_p_ff: Some(b"unix".to_vec()),
            b_start_ffc: i32::from(b'u'),
            b_p_fenc: Some(b"utf-8".to_vec()),
            b_start_fenc: Some(b"utf-8".to_vec()),
            ..Default::default()
        };
        assert!(!unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn file_ff_differs_true_when_fileencoding_changed() {
        let mut buf = BufT {
            b_p_ff: Some(b"unix".to_vec()),
            b_start_ffc: i32::from(b'u'),
            b_p_fenc: Some(b"latin1".to_vec()),
            b_start_fenc: Some(b"utf-8".to_vec()),
            ..Default::default()
        };
        assert!(unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn file_ff_differs_true_when_bomb_changed_and_not_binary() {
        let mut buf = BufT {
            b_p_ff: Some(b"unix".to_vec()),
            b_start_ffc: i32::from(b'u'),
            b_p_bin: 0,
            b_p_bomb: 1,
            b_start_bomb: 0,
            ..Default::default()
        };
        assert!(unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn file_ff_differs_ignores_bomb_change_when_binary() {
        let mut buf = BufT {
            b_p_ff: Some(b"unix".to_vec()),
            b_start_ffc: i32::from(b'u'),
            b_p_bin: 1,
            b_p_bomb: 1,
            b_start_bomb: 0,
            ..Default::default()
        };
        assert!(!unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn changed_internal_sets_b_changed_and_redraw_bookkeeping() {
        // Touches shared GLOBALS.redraw_tabline/need_maketitle.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_changed: 0, ..Default::default() };
        unsafe { crate::globals::GLOBALS.get_mut() }.redraw_tabline = false;
        unsafe { crate::globals::GLOBALS.get_mut() }.need_maketitle = false;

        unsafe { changed_internal(&mut buf as *mut BufT) };

        assert_eq!(buf.b_changed, 1);
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.redraw_tabline);
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.need_maketitle);
    }

    #[test]
    fn changed_internal_does_not_panic_when_already_changed() {
        // was_changed == true skips the aucmd_defer_modified call
        // entirely - verify calling it again on an already-changed
        // buffer is still a clean no-op (AUTOCMDS[OptionSet] is empty
        // either way, but the was_changed branch itself must not
        // panic or misbehave).
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_changed: 1, ..Default::default() };
        unsafe { changed_internal(&mut buf as *mut BufT) };
        assert_eq!(buf.b_changed, 1);
    }

    #[test]
    fn unchanged_resets_changed_flag_and_saves_fileformat_when_ff_true() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT {
            b_changed: 1,
            b_p_ff: Some(b"dos".to_vec()),
            b_start_ffc: i32::from(b'x'), // stale, must be refreshed
            ..Default::default()
        };
        let before_tick = buf_get_changedtick(&buf);

        unsafe { unchanged(&mut buf as *mut BufT, true, false) };

        assert_eq!(buf.b_changed, 0);
        // save_file_ff was called for real: b_start_ffc now matches
        // 'fileformat's own first byte.
        assert_eq!(buf.b_start_ffc, i32::from(b'd'));
        assert_eq!(buf_get_changedtick(&buf), before_tick + 1);
    }

    #[test]
    fn unchanged_does_not_save_fileformat_when_ff_is_false() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT {
            b_changed: 1,
            b_p_ff: Some(b"dos".to_vec()),
            b_start_ffc: i32::from(b'x'),
            ..Default::default()
        };

        unsafe { unchanged(&mut buf as *mut BufT, false, false) };

        assert_eq!(buf.b_changed, 0);
        // save_file_ff was NOT called: the stale b_start_ffc survives.
        assert_eq!(buf.b_start_ffc, i32::from(b'x'));
    }

    #[test]
    fn unchanged_triggers_via_file_ff_differs_even_when_not_changed() {
        let _lock = crate::globals::global_state_test_lock();
        // b_changed starts false, but 'fileformat' genuinely differs
        // from what was saved when editing started - ff_differs alone
        // must be enough to enter the real branch.
        let mut buf = BufT {
            b_changed: 0,
            b_p_ff: Some(b"dos".to_vec()),
            b_start_ffc: i32::from(b'u'),
            ..Default::default()
        };
        let before_tick = buf_get_changedtick(&buf);

        unsafe { unchanged(&mut buf as *mut BufT, true, false) };

        assert_eq!(buf.b_start_ffc, i32::from(b'd'));
        assert_eq!(buf_get_changedtick(&buf), before_tick + 1);
    }

    #[test]
    fn unchanged_increments_changedtick_via_always_inc_changedtick_only() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_changed: 0, ..Default::default() };
        unsafe { crate::globals::GLOBALS.get_mut() }.redraw_tabline = false;
        let before_tick = buf_get_changedtick(&buf);

        // ff == false, so ff_differs is false too - the `else if`
        // branch is the only one that can fire here.
        unsafe { unchanged(&mut buf as *mut BufT, false, true) };

        assert_eq!(buf_get_changedtick(&buf), before_tick + 1);
        // The main branch's own bookkeeping must NOT have run.
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.redraw_tabline);
    }

    #[test]
    fn unchanged_is_a_complete_noop_when_nothing_triggers() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_changed: 0, ..Default::default() };
        unsafe { crate::globals::GLOBALS.get_mut() }.redraw_tabline = false;
        let before_tick = buf_get_changedtick(&buf);

        unsafe { unchanged(&mut buf as *mut BufT, false, false) };

        assert_eq!(buf_get_changedtick(&buf), before_tick);
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.redraw_tabline);
    }

    #[test]
    fn save_file_ff_snapshots_every_tracked_option() {
        let mut buf = BufT {
            b_p_ff: Some(b"dos".to_vec()),
            b_p_eof: 1,
            b_p_eol: 0,
            b_p_bomb: 1,
            b_p_fenc: Some(b"latin1".to_vec()),
            ..Default::default()
        };
        save_file_ff(&mut buf);
        assert_eq!(buf.b_start_ffc, i32::from(b'd'));
        assert_eq!(buf.b_start_eof, 1);
        assert_eq!(buf.b_start_eol, 0);
        assert_eq!(buf.b_start_bomb, 1);
        assert_eq!(buf.b_start_fenc, Some(b"latin1".to_vec()));
    }

    #[test]
    fn save_file_ff_with_none_fileformat_and_fileencoding_defaults_to_zero_and_none() {
        let mut buf = BufT {
            b_p_ff: None,
            b_p_fenc: None,
            b_start_ffc: i32::from(b'x'),
            b_start_fenc: Some(b"stale".to_vec()),
            ..Default::default()
        };
        save_file_ff(&mut buf);
        assert_eq!(buf.b_start_ffc, 0);
        assert_eq!(buf.b_start_fenc, None);
    }

    #[test]
    fn save_file_ff_then_file_ff_differs_reports_no_difference() {
        let mut buf = BufT {
            b_p_ff: Some(b"unix".to_vec()),
            b_p_eof: 1,
            b_p_eol: 0,
            b_p_bomb: 0,
            b_p_fenc: Some(b"utf-8".to_vec()),
            ..Default::default()
        };
        save_file_ff(&mut buf);
        assert!(!unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn save_file_ff_then_file_ff_differs_reports_difference_after_fenc_change() {
        let mut buf = BufT {
            b_p_ff: Some(b"unix".to_vec()),
            b_p_fenc: Some(b"utf-8".to_vec()),
            ..Default::default()
        };
        save_file_ff(&mut buf);
        // Simulate the user changing 'fileencoding' after editing
        // started.
        buf.b_p_fenc = Some(b"latin1".to_vec());
        assert!(unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn save_file_ff_then_file_ff_differs_reports_difference_after_fileformat_change() {
        let mut buf = BufT { b_p_ff: Some(b"unix".to_vec()), ..Default::default() };
        save_file_ff(&mut buf);
        // Simulate the user changing 'fileformat' after editing
        // started.
        buf.b_p_ff = Some(b"dos".to_vec());
        assert!(unsafe { file_ff_differs(&mut buf, false) });
    }

    /// Points `GLOBALS.curbuf` at `buf` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime (this
    /// guard does NOT acquire its own lock - matching this crate's
    /// established "compose with an externally-held lock" pattern for
    /// guards that need to combine with other shared-state setup).
    struct CurbufGuard {
        previous: *mut BufT,
    }

    impl CurbufGuard {
        fn set(new_curbuf: *mut BufT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = new_curbuf;
            CurbufGuard { previous }
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.previous;
        }
    }

    #[test]
    fn change_warning_is_a_noop_when_buffer_not_readonly() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_ro: 0, ..Default::default() };
        let buf_ptr = &mut buf as *mut BufT;
        let _guard = CurbufGuard::set(buf_ptr);

        unsafe { change_warning(buf_ptr, 0) };

        assert!(!buf.b_did_warn);
    }

    #[test]
    fn change_warning_is_a_noop_when_already_warned() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_ro: 1, b_did_warn: true, ..Default::default() };
        let buf_ptr = &mut buf as *mut BufT;
        let _guard = CurbufGuard::set(buf_ptr);
        unsafe { crate::globals::GLOBALS.get_mut() }.redraw_cmdline = true;

        unsafe { change_warning(buf_ptr, 0) };

        // Unchanged: change_warning's own guard condition requires
        // b_did_warn == false to do anything at all.
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.redraw_cmdline);
    }

    #[test]
    fn change_warning_is_a_noop_when_autocmd_busy() {
        // Not achievable via any real translated function yet (nothing
        // can set AUTOCMD_BUSY true) - pokes it directly to prove
        // change_warning's own `!autocmd_busy` guard condition is
        // faithfully translated, independent of how AUTOCMD_BUSY
        // eventually gets set.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_ro: 1, ..Default::default() };
        let buf_ptr = &mut buf as *mut BufT;
        let _guard = CurbufGuard::set(buf_ptr);
        unsafe { *crate::autocmd::AUTOCMD_BUSY.get_mut() = true };

        unsafe { change_warning(buf_ptr, 0) };

        unsafe { *crate::autocmd::AUTOCMD_BUSY.get_mut() = false };
        assert!(!buf.b_did_warn);
    }

    #[test]
    fn change_warning_is_a_noop_when_buffer_already_changed() {
        let _lock = crate::globals::global_state_test_lock();
        // b_changed != 0 makes curbuf_is_changed() true (bt_dontwrite
        // is false for a plain, default buftype).
        let mut buf = BufT { b_p_ro: 1, b_changed: 1, ..Default::default() };
        let buf_ptr = &mut buf as *mut BufT;
        let _guard = CurbufGuard::set(buf_ptr);

        unsafe { change_warning(buf_ptr, 0) };

        assert!(!buf.b_did_warn);
    }

    #[test]
    fn change_warning_warns_once_for_an_unchanged_readonly_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_ro: 1, ..Default::default() };
        let buf_ptr = &mut buf as *mut BufT;
        let _guard = CurbufGuard::set(buf_ptr);
        unsafe { crate::globals::GLOBALS.get_mut() }.redraw_cmdline = true;

        unsafe { change_warning(buf_ptr, 0) };

        assert!(buf.b_did_warn);
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.redraw_cmdline);
        // b_ro_locked is incremented then decremented around the
        // apply_autocmds call - net zero once change_warning returns.
        assert_eq!(buf.b_ro_locked, 0);
        assert_eq!(
            unsafe { crate::eval::vars::get_vim_var_str(crate::eval::vars::VimVarIndex::Warningmsg) },
            b"W10: Warning: Changing a readonly file"
        );

        // A second call is now a no-op (b_did_warn short-circuits).
        unsafe { crate::globals::GLOBALS.get_mut() }.redraw_cmdline = true;
        unsafe { change_warning(buf_ptr, 0) };
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.redraw_cmdline);

        // Reset: VIMVARS is shared, process-wide state.
        unsafe {
            crate::eval::vars::set_vim_var_string(crate::eval::vars::VimVarIndex::Warningmsg, None)
        };
    }

    #[test]
    fn change_warning_leaves_warningmsg_untouched_when_a_noop() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_ro: 0, ..Default::default() };
        let buf_ptr = &mut buf as *mut BufT;
        let _guard = CurbufGuard::set(buf_ptr);

        unsafe { change_warning(buf_ptr, 0) };

        assert_eq!(
            unsafe { crate::eval::vars::get_vim_var_str(crate::eval::vars::VimVarIndex::Warningmsg) },
            Vec::<u8>::new()
        );
    }

    // ---- changed ----

    #[test]
    fn changed_marks_the_buffer_as_changed_and_increments_changedtick() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_changed: 0, ..Default::default() };
        let buf_ptr = &mut buf as *mut BufT;
        let _guard = CurbufGuard::set(buf_ptr);
        let before_tick = buf_get_changedtick(&buf);

        unsafe { changed(buf_ptr) };

        assert_eq!(buf.b_changed, 1);
        assert_eq!(buf_get_changedtick(&buf), before_tick + 1);
    }

    #[test]
    fn changed_still_increments_changedtick_when_already_changed() {
        // buf_inc_changedtick() runs unconditionally, outside the
        // `if !b_changed` block - a second `changed()` call still
        // bumps the tick even though change_warning/changed_internal
        // are both skipped.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_changed: 1, ..Default::default() };
        let buf_ptr = &mut buf as *mut BufT;
        let _guard = CurbufGuard::set(buf_ptr);
        let before_tick = buf_get_changedtick(&buf);

        unsafe { changed(buf_ptr) };

        assert_eq!(buf.b_changed, 1);
        assert_eq!(buf_get_changedtick(&buf), before_tick + 1);
    }

    #[test]
    fn changed_resets_search_hl_match() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_changed: 0, ..Default::default() };
        let buf_ptr = &mut buf as *mut BufT;
        let _guard = CurbufGuard::set(buf_ptr);
        unsafe { crate::globals::GLOBALS.get_mut() }.Search.hl_match = true;

        unsafe { changed(buf_ptr) };

        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.Search.hl_match);
    }

    #[test]
    #[should_panic(expected = "change::changed: creating a swap file needs ml_open_file")]
    fn changed_panics_if_b_may_swap_is_ever_genuinely_true() {
        // Not achievable via any real translated function yet (nothing
        // can set OPTION_VARS.p_uc != 0 / BufT.b_may_swap true) - pokes
        // it directly to prove the real, always-false-today check is
        // faithfully translated, independent of how b_may_swap
        // eventually gets set for real.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_changed: 0, b_may_swap: true, ..Default::default() };
        let buf_ptr = &mut buf as *mut BufT;
        let _guard = CurbufGuard::set(buf_ptr);

        unsafe { changed(buf_ptr) };
    }



    fn buf_with_com(com: &[u8]) -> BufT {
        BufT { b_p_com: Some(com.to_vec()), ..Default::default() }
    }

    #[test]
    fn get_leader_len_matches_a_simple_two_char_comment() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_com(b"://");
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        assert_eq!(unsafe { get_leader_len(b"// hello", None, false, false) }, 2);
        assert_eq!(unsafe { get_leader_len(b"// hello", None, false, true) }, 3);
    }

    #[test]
    fn get_leader_len_no_match_returns_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_com(b"://");
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        assert_eq!(unsafe { get_leader_len(b"hello", None, false, false) }, 0);
    }

    #[test]
    fn get_leader_len_nested_marker_accumulates_across_levels() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_com(b"n:>");
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        // The 'n' (nest) flag lets the SAME comment marker repeat -
        // both '>' characters are recognized as one combined leader.
        assert_eq!(unsafe { get_leader_len(b">> quoted text", None, false, false) }, 2);
        // A lone '>' followed by non-matching text still just matches
        // the one level available.
        assert_eq!(unsafe { get_leader_len(b"> quoted text", None, false, false) }, 1);
    }

    #[test]
    fn get_leader_len_com_blank_flag_requires_trailing_whitespace() {
        let _lock = crate::globals::global_state_test_lock();
        // Two parts: "a:X" (rejected - doesn't match the line) then
        // "b:Y" (the 'b' flag requires trailing whitespace after "Y").
        let mut buf = buf_with_com(b"a:X,b:Y");
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        assert_eq!(unsafe { get_leader_len(b"Y trailing", None, false, false) }, 1);
        // No whitespace after "Y" - COM_BLANK rejects the match.
        assert_eq!(unsafe { get_leader_len(b"Ytrailing", None, false, false) }, 0);
    }

    #[test]
    fn get_leader_len_sets_flags_to_the_matching_parts_offset() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_com(b"a:X,b:Y");
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        let mut flags_offset = usize::MAX;
        let result = unsafe { get_leader_len(b"Y trailing", Some(&mut flags_offset), false, false) };
        assert_eq!(result, 1);
        // "b:Y" starts at offset 4 in "a:X,b:Y" - the part that
        // actually matched, not the earlier rejected "a:X".
        assert_eq!(flags_offset, 4);
    }

    #[test]
    fn get_leader_len_falls_back_to_a_middle_match_when_no_better_end_match() {
        let _lock = crate::globals::global_state_test_lock();
        // "m:*" (middle) at offset 0, "e:*/" (end) at offset 4. A lone
        // "*" with no following "/" matches the middle definition but
        // not the (longer) end definition.
        let mut buf = buf_with_com(b"m:*,e:*/");
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        let mut flags_offset = usize::MAX;
        let result = unsafe { get_leader_len(b"* text", Some(&mut flags_offset), false, false) };
        assert_eq!(result, 1);
        // flags is restored to the MIDDLE match's own offset (0), not
        // the failed end-match attempt's offset (4).
        assert_eq!(flags_offset, 0);
    }

    #[test]
    fn get_leader_len_backward_skips_noback_flagged_parts() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_com(b"O:X");
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        assert_eq!(unsafe { get_leader_len(b"X trailing", None, false, false) }, 1);
        // backward=true skips any part flagged 'O' (COM_NOBACK).
        assert_eq!(unsafe { get_leader_len(b"X trailing", None, true, false) }, 0);
    }

    // ---- get_last_leader_offset ----

    #[test]
    fn get_last_leader_offset_finds_a_trailing_comment() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_com(b"://");
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        // "code // comment" - the "//" pair starts at byte offset 5.
        assert_eq!(unsafe { get_last_leader_offset(b"code // comment", None) }, Some(5));
    }

    #[test]
    fn get_last_leader_offset_no_comment_returns_none() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_com(b"://");
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        assert_eq!(unsafe { get_last_leader_offset(b"just code here", None) }, None);
    }

    #[test]
    fn get_last_leader_offset_sets_flags_to_the_matching_parts_offset() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_com(b"://");
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        let mut flags_offset = usize::MAX;
        let result = unsafe { get_last_leader_offset(b"code // comment", Some(&mut flags_offset)) };
        assert_eq!(result, Some(5));
        assert_eq!(flags_offset, 0);
    }

    #[test]
    fn get_last_leader_offset_substring_verification_prefers_the_longer_overlapping_leader() {
        let _lock = crate::globals::global_state_test_lock();
        // Two comment definitions: "ab" (offset 0) and "b" (offset 4).
        // "b" is a suffix of "ab" - without the substring-verification
        // pass adjusting lower_check_bound, the backward scan would
        // stop at the SHORTER "b" match (byte offset 2) and never look
        // further back to find the real, longer "ab" leader starting
        // at byte offset 1.
        let mut buf = buf_with_com(b":ab,:b");
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        let mut flags_offset = usize::MAX;
        let result = unsafe { get_last_leader_offset(b"xab", Some(&mut flags_offset)) };
        assert_eq!(result, Some(1));
        // flags now correctly reflects the LONGER "ab" leader's own
        // offset (0), not the shorter "b" leader's offset (4).
        assert_eq!(flags_offset, 0);
    }

    #[test]
    fn get_last_leader_offset_nested_marker_finds_the_leftmost_level() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_com(b"n:>");
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        // The 'n' (nest) flag lets the scan keep looking further back
        // for more nested markers - the LEFTMOST (second) '>' is the
        // true start of the nested-marker run.
        assert_eq!(unsafe { get_last_leader_offset(b"text >>", None) }, Some(5));
    }
}
