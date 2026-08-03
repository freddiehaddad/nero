//! Translated from `src/nvim/change.c` (partial).
//!
//! `change.c` (~2200 lines) is the buffer-modification/change-tracking
//! core (`changed`/`changed_bytes`/`changed_lines`, insert-mode byte
//! insertion, indent-preservation helpers, etc.). Re-examined after
//! `memline.c`'s write side (`ml_replace`/`ml_append`/`ml_delete`) and
//! `autocmd.c`'s `apply_autocmds` (real, faithful "no autocmds
//! registered" bypass path) were both completed - `change_warning` is
//! now tractable too, since `autocmd_busy` is a real, always-`false`
//! global (see `crate::autocmd::AUTOCMD_BUSY`'s own doc comment) and
//! `apply_autocmds` itself is real. `changed`/`changed_internal`/
//! `changed_common`/`changed_lines_invalidate_win` (etc.) still need a
//! wide spread of OTHER not-yet-translated subsystems though:
//! `ml_open_file` (swap-file creation), window/fold display
//! bookkeeping (`redraw_buf_status_later`, `find_wl_entry`,
//! `invalidate_botline_win`, `buf_meta_total`), `diff_internal`/
//! `diff_update_line` (`diff.c`), and `buf_inc_changedtick` (the real
//! `b:` dict watcher machinery, eval engine/phase 5).
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
//! Deferred: everything else in the file - each is its own substantial
//! undertaking blocked on subsystems not yet translated (the display
//! pipeline, the fold/diff subsystems, the eval engine's `b:` dict
//! watchers, etc. - see above).

use crate::ascii_defs::ascii_iswhite;
use crate::buffer_defs::{b_flags, BufT};
use crate::option::copy_option_part;
use crate::strings::vim_strchr;

/// Internal part of `changed()`, no user interaction (`changed_internal`).
/// Also used for recovery.
///
/// Every dependency this function needs (`ml_setflags`,
/// `redraw_buf_status_later`, `GLOBALS.redraw_tabline`/
/// `need_maketitle`, `aucmd_defer_modified` - always a no-op today,
/// see its own doc comment) is now real, unlike its own caller
/// `changed()` (still needs `ml_open_file`/the message-display
/// pipeline for its OWN body, so `changed()` itself remains deferred -
/// see this module's own doc comment).
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
            if !got_com {
                if let Some(f) = flags.as_mut() {
                    **f = list; // remember where flags started
                }
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
            if !got_com {
                if let (Some(f), Some(sf)) = (flags.as_mut(), saved_flags) {
                    **f = sf;
                }
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

    // ---- get_leader_len ----

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
