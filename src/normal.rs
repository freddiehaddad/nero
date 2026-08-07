//! Translated from `src/nvim/normal.c` (tractable core only).
//!
//! `normal.c` (~6600 lines) is the Normal-mode command-dispatch engine
//! (the giant `normal_cmd`/`nv_*` handler table) - almost none of it
//! is tractable, since it needs real buffer modification, the redraw
//! pipeline, the regex engine, and the whole rest of the editing
//! subsystem, none of which are translated yet.
//!
//! Translated: [`is_ident`] - a small, pure, self-contained C-style-
//! comment/string-literal scanner. Translated ahead of its own real
//! caller (`find_decl`, the `"gd"`/`"gD"` variable-declaration search,
//! not translated - needs `find_ident_under_cursor`/`searchit`, the
//! real regex engine), matching this crate's established "small,
//! self-contained, no design freedom to get wrong" precedent.
//!
//! Deferred: everything else in the file.

/// Clear a pending operator (`clearop`).
///
/// Resets both the operator argument's own fields AND the global
/// `motion_force`, which is separate state the caller would otherwise
/// have to remember to clear itself.
///
/// # Safety
/// Must not run concurrently with any other access to
/// `crate::globals::GLOBALS`.
pub unsafe fn clearop(oap: &mut crate::normal_defs::OpargT) {
    oap.op_type = crate::ops_defs::OpType::Nop as i32;
    oap.regname = 0;
    oap.motion_force = i32::from(crate::ascii_defs::NUL);
    oap.use_reg_one = false;
    oap.restore_cursor = false;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::globals::GLOBALS.get_mut() }.motion_force = i32::from(crate::ascii_defs::NUL);
}

/// Rewrite a shifted cursor key in `cap` to its unshifted form
/// (`unshift_special`).
///
/// The shift is not simply discarded: `simplify_key` folds it back
/// into the global `mod_mask`, so a mapping can still see it.
///
/// # Safety
/// Must not run concurrently with any other access to
/// `crate::globals::GLOBALS` (touches `mod_mask`).
pub unsafe fn unshift_special(cap: &mut crate::normal_defs::CmdargT) {
    use crate::keycodes_defs as kc;
    cap.cmdchar = match cap.cmdchar {
        c if c == kc::K_S_RIGHT => kc::K_RIGHT,
        c if c == kc::K_S_LEFT => kc::K_LEFT,
        c if c == kc::K_S_UP => kc::K_UP,
        c if c == kc::K_S_DOWN => kc::K_DOWN,
        c if c == kc::K_S_HOME => kc::K_HOME,
        c if c == kc::K_S_END => kc::K_END,
        other => other,
    };
    // SAFETY: forwarded from this function's own safety doc.
    let mod_mask = &mut unsafe { crate::globals::GLOBALS.get_mut() }.mod_mask;
    cap.cmdchar = crate::keycodes::simplify_key(cap.cmdchar, mod_mask);
}

/// Whether the current buffer's `'comments'` option defines a C-style
/// (`//` or `/*`) comment leader (`buf_has_cstyle_comments`).
///
/// Each comma-separated part of `'comments'` is `flags:leader`; this
/// looks for a leader starting `/` followed by `/` or `*`.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
#[must_use]
pub unsafe fn buf_has_cstyle_comments() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let com = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf }
        .b_p_com
        .clone()
        .unwrap_or_default();

    let mut list = 0usize;
    while list < com.len() && com[list] != crate::ascii_defs::NUL {
        let (part_buf, next) = crate::option::copy_option_part(
            &com,
            list,
            crate::option_vars::COM_MAX_LEN as usize,
            b",",
        );
        list = next;
        // Flags and comment leader are separated by a colon.
        if let Some(colon) = crate::strings::vim_strchr(&part_buf, i32::from(b':'))
            && part_buf.get(colon + 1) == Some(&b'/')
            && matches!(part_buf.get(colon + 2), Some(&b'/') | Some(&b'*'))
        {
            return true;
        }
    }
    false
}

/// Returns `true` if `line[offset]` is NOT inside a C-style comment or
/// string, `false` otherwise (`is_ident`).
///
/// Assumes `line` is a well-formed line (this crate's own convention:
/// includes its own trailing NUL) - running out of a malformed,
/// non-NUL-terminated slice before reaching `offset` is treated the
/// same way as hitting the terminator, matching `mbyte.c`/`indent.c`'s
/// established "ran out of slice = terminator" precedent.
#[must_use]
pub fn is_ident(line: &[u8], offset: i32) -> bool {
    let mut incomment = false;
    let mut instring: u8 = 0;
    let mut prev: u8 = 0;

    let offset = offset.max(0) as usize;
    let mut i = 0usize;
    while i < offset {
        let Some(&c) = line.get(i) else { break };
        if c == 0 {
            break;
        }

        if instring != 0 {
            if prev != b'\\' && c == instring {
                instring = 0;
            }
        } else if (c == b'"' || c == b'\'') && !incomment {
            instring = c;
        } else if incomment {
            if prev == b'*' && c == b'/' {
                incomment = false;
            }
        } else if prev == b'/' && c == b'*' {
            incomment = true;
        } else if prev == b'/' && c == b'/' {
            return false;
        }

        prev = c;
        i += 1;
    }

    !incomment && instring == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- unshift_special / buf_has_cstyle_comments ---

    #[test]
    fn unshift_special_maps_each_shifted_cursor_key_to_its_plain_form() {
        let _lock = crate::globals::global_state_test_lock();
        use crate::keycodes_defs as kc;
        for (shifted, plain) in [
            (kc::K_S_RIGHT, kc::K_RIGHT),
            (kc::K_S_LEFT, kc::K_LEFT),
            (kc::K_S_UP, kc::K_UP),
            (kc::K_S_DOWN, kc::K_DOWN),
            (kc::K_S_HOME, kc::K_HOME),
            (kc::K_S_END, kc::K_END),
        ] {
            let mut cap = crate::normal_defs::CmdargT { cmdchar: shifted, ..Default::default() };
            unsafe { unshift_special(&mut cap) };
            assert_eq!(cap.cmdchar, plain);
        }
    }

    #[test]
    fn unshift_special_leaves_an_unshifted_key_alone() {
        let _lock = crate::globals::global_state_test_lock();
        let mut cap =
            crate::normal_defs::CmdargT { cmdchar: i32::from(b'x'), ..Default::default() };
        unsafe { unshift_special(&mut cap) };
        assert_eq!(cap.cmdchar, i32::from(b'x'));
    }

    #[test]
    fn buf_has_cstyle_comments_finds_a_slash_leader() {
        // Cross-verified against real nvim: the default 'comments'
        // contains both "s1:/*" and "://".
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT {
            b_p_com: Some(b"s1:/*,mb:*,ex:*/,://,b:#".to_vec()),
            ..Default::default()
        };
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.curbuf;
        g.curbuf = &mut buf;

        assert!(unsafe { buf_has_cstyle_comments() });

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev;
    }

    #[test]
    fn buf_has_cstyle_comments_is_false_without_one() {
        let _lock = crate::globals::global_state_test_lock();
        // Hash and quote leaders only - no `/` followed by `/` or `*`.
        let mut buf = crate::buffer_defs::BufT {
            b_p_com: Some(b"b:#,:%,n:>,fb:-".to_vec()),
            ..Default::default()
        };
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.curbuf;
        g.curbuf = &mut buf;

        assert!(!unsafe { buf_has_cstyle_comments() });

        // An empty 'comments' likewise has nothing to find.
        unsafe { (*crate::globals::GLOBALS.get_mut().curbuf).b_p_com = Some(Vec::new()) };
        assert!(!unsafe { buf_has_cstyle_comments() });

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev;
    }

    #[test]
    fn buf_has_cstyle_comments_needs_the_slash_right_after_the_colon() {
        // A leader of "*" alone must not count, even though a
        // C-comment continuation uses it.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT {
            b_p_com: Some(b"mb:*,ex:*/".to_vec()),
            ..Default::default()
        };
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.curbuf;
        g.curbuf = &mut buf;

        assert!(!unsafe { buf_has_cstyle_comments() });

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev;
    }

    #[test]
    fn clearop_resets_every_operator_field() {
        let _lock = crate::globals::global_state_test_lock();
        let mut oap = crate::normal_defs::OpargT {
            op_type: crate::ops_defs::OpType::Delete as i32,
            regname: i32::from(b'a'),
            motion_force: i32::from(b'v'),
            use_reg_one: true,
            restore_cursor: true,
            ..Default::default()
        };

        unsafe { clearop(&mut oap) };

        assert_eq!(oap.op_type, crate::ops_defs::OpType::Nop as i32);
        assert_eq!(oap.regname, 0);
        assert_eq!(oap.motion_force, 0);
        assert!(!oap.use_reg_one);
        assert!(!oap.restore_cursor);
    }

    #[test]
    fn clearop_also_clears_the_global_motion_force() {
        let _lock = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = globals.motion_force;
        globals.motion_force = i32::from(b'v');

        let mut oap = crate::normal_defs::OpargT::default();
        unsafe { clearop(&mut oap) };

        // The global is separate state from the oparg's own field, so
        // clearing only the latter would leave a stale force behind.
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.motion_force, 0);

        unsafe { crate::globals::GLOBALS.get_mut() }.motion_force = prev;
    }

    #[test]
    fn clearop_on_an_already_clear_oparg_is_idempotent() {
        let _lock = crate::globals::global_state_test_lock();
        let mut oap = crate::normal_defs::OpargT::default();
        unsafe { clearop(&mut oap) };
        unsafe { clearop(&mut oap) };
        assert_eq!(oap.op_type, crate::ops_defs::OpType::Nop as i32);
    }

    #[test]
    fn is_ident_plain_code_before_offset_is_true() {
        assert!(is_ident(b"int x = 5;\0", 5));
    }

    #[test]
    fn is_ident_inside_a_double_quoted_string_is_false() {
        // offset=6 lands right after the opening quote, inside "hi".
        assert!(!is_ident(b"x = \"hi\";\0", 6));
    }

    #[test]
    fn is_ident_after_a_closed_string_is_true() {
        // offset=9 is right after the closing quote - the string has
        // ended, so this position is NOT inside it.
        assert!(is_ident(b"x = \"hi\";\0", 9));
    }

    #[test]
    fn is_ident_inside_a_single_quoted_string_is_false() {
        assert!(!is_ident(b"c = 'x';\0", 5));
    }

    #[test]
    fn is_ident_an_escaped_quote_does_not_close_the_string() {
        // `"a\"b"` bytes: 0='"',1='a',2='\\',3='"',4='b',5='"',6=NUL.
        // The backslash-escaped quote at index 3 must NOT close the
        // string; offset=4 (the 'b') is still inside it.
        assert!(!is_ident(b"\"a\\\"b\"\0", 4));
    }

    #[test]
    fn is_ident_inside_a_block_comment_is_false() {
        assert!(!is_ident(b"/* comment */ x\0", 5));
    }

    #[test]
    fn is_ident_after_a_closed_block_comment_is_true() {
        assert!(is_ident(b"/* c */ x\0", 8));
    }

    #[test]
    fn is_ident_a_line_comment_makes_everything_after_it_false() {
        // Once `//` is seen, the function returns false immediately
        // (a line comment runs to the end of the line - there is no
        // "closing" it within the same line).
        assert!(!is_ident(b"x // comment\0", 4));
        assert!(!is_ident(b"x // comment\0", 12));
    }

    #[test]
    fn is_ident_offset_zero_is_always_true() {
        // The loop never runs at all - nothing has been scanned yet,
        // so we're trivially "not inside" anything.
        assert!(is_ident(b"\"unterminated\0", 0));
    }

    #[test]
    fn is_ident_stops_at_a_truncated_non_nul_terminated_slice() {
        // No NUL terminator at all - running out of the slice before
        // reaching `offset` is treated the same as hitting one.
        assert!(is_ident(b"abc", 10));
    }
}
