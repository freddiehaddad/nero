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
//! Also [`cin_is_cinword`]: the note below previously listed it as
//! blocked on `vim_iswordc`, but that function has since landed in
//! `charset.rs`, so the note was stale. It needs only `skipwhite`,
//! `copy_option_part` and the buffer's own `'cinwords'`.
//!
//! Also [`skip_string`] and [`is_pos_in_string`] - both pure, needing
//! only `ascii_isdigit` and `vim_strchr`. Two real quirks in the
//! original are preserved and pinned by tests: an UNTERMINATED string
//! does not leave the index "unmodified" as the header comment
//! suggests (the opening quote has already advanced it), and the
//! octal char-constant path is unreachable because its own digit loop
//! overshoots by one.
//!
//! Also [`cin_skipcomment`] and [`cin_nocode`] - the whitespace and
//! comment skipper the rest of the indent engine is built on. Needs
//! only `skipwhite` and the buffer's own `b_ind_hash_comment`.
//!
//! Also [`cin_islabel_skip`] and [`cin_has_js_key`], both unlocked by
//! `cin_skipcomment`. `cin_islabel_skip` returns `Option<usize>`
//! rather than the original's bool-plus-in-place-advance `const char
//! **s`.
//!
//! Also [`cin_iscase`], [`cin_isdefault`] and [`cin_isscopedecl`] -
//! the switch-label and `'cinscopedecls'` recognizers, all resting on
//! `cin_skipcomment`.
//!
//! Also [`cin_skip_comment_and_string`] and [`cin_is_compound_init`]
//! (structure/compound-literal initialization: `=|return
//! [&][(typecast)] [{]`).
//!
//! Also the small keyword/shape predicates [`cin_ispreproc`],
//! [`cin_iscomment`], [`cin_islinecomment`], [`cin_isif`],
//! [`cin_iselse`], [`cin_isdo`], [`cin_isbreak`] and [`cin_ends_in`].
//!
//! Also [`cin_is_cpp_extern_c`] (`extern "C"`/`extern "C++"` linkage
//! specifications). Note its keyword-boundary test uses `vim_iswordc`
//! rather than the `vim_isIDc` [`cin_starts_with`] uses, so it is
//! deliberately NOT expressed through that helper.
//!
//! Also [`cin_isterminated`] - whether a line starts with `'{'`/`'}'`
//! or ends with `';'`/`','`/`'{'`/`'}'`, returning the terminating
//! character itself. Two of its rules are subtle and are pinned by
//! tests: `"} else"` is deliberately NOT terminated, and after a
//! leading `"else"` a terminator only counts once any braces opened
//! on the same line have been matched (so `"else { foo();"` is not
//! terminated but `"else { foo(); }"` is).
//!
//! Also [`cin_is_cpp_namespace`] (with its optional `"inline"`/
//! `"export"` prefixes and C++17 nested `a::b` names) and
//! [`after_label`], which returns `Option<usize>` in place of the
//! original's nullable `const char *`.
//!
//! Also [`cin_is_if_for_while_before_offset`], which returns
//! `Option<usize>` in place of the original's bool-plus-in/out
//! `int *poffset`. Its whitespace-skipping loop has a STRICT
//! `offset > 2` bound, so a keyword at column 0 followed by a space
//! is missed while the same text indented is found - preserved and
//! pinned by its own test rather than "fixed".
//!
//! Also [`check_linecomment`] - the column of a line comment's start,
//! or `MAXCOL` for none. Handles both the C `//` form and, for a
//! `'lisp'` buffer, the `;` form with its own string tracking.
//!
//! Deferred: everything else - `cin_ispreproc_cont`/
//! `find_line_comment`/`cin_iswhileofdo_end` (need `ml_get`, the
//! cursor and `find_match_paren`) and the rest of the real
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

/// Whether `line` starts with a word from `'cinwords'`
/// (`cin_is_cinword`).
///
/// The original allocates a scratch buffer sized to `'cinwords'` and
/// hands it to `copy_option_part`; this crate's `copy_option_part`
/// returns the part it extracted, so no scratch buffer is needed.
///
/// Note the trailing check is `!vim_iswordc(line[len]) ||
/// !vim_iswordc(line[len - 1])` - an OR, not the `&&` a reader might
/// expect. So a `'cinwords'` entry that itself ENDS in a
/// non-word character matches even when the text continues with a
/// word character. Preserved exactly, and pinned by its own test.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`, matching [`cindent_on`]'s own safety doc.
#[must_use]
pub unsafe fn cin_is_cinword(line: &[u8]) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let cinw = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf }
        .b_p_cinw
        .clone()
        .unwrap_or_default();
    let cinw_len = cinw.len() + 1;

    let start = crate::charset::skipwhite(line);
    let line = &line[start..];

    let mut p = 0usize;
    while p < cinw.len() {
        let (part, next) = crate::option::copy_option_part(&cinw, p, cinw_len, b",");
        p = next;
        let len = part.len();
        if len == 0 {
            continue;
        }
        if line.starts_with(&part) {
            // SAFETY: forwarded from this function's own safety doc -
            // `vim_iswordc` reads the same `curbuf` this one does.
            let (after_is_word, last_is_word) = unsafe {
                (
                    line.get(len)
                        .is_some_and(|&c| crate::charset::vim_iswordc(i32::from(c))),
                    crate::charset::vim_iswordc(i32::from(part[len - 1])),
                )
            };
            if !after_is_word || !last_is_word {
                return true;
            }
        }
    }

    false
}

/// Skip to the end of a `"string"` or a `'c'` character constant
/// (`skip_string`). If there is no string or character at `p`, `p` is
/// returned unmodified.
///
/// The original takes and returns a `const char *`; here `p` is a byte
/// INDEX into `line`, matching this crate's established
/// index-instead-of-pointer convention.
///
/// Note the original's trailing `if (!*p) p--;` "backup from NUL":
/// when the scan runs off the end of the line it steps back one byte
/// so the caller's own `p++` lands exactly on the terminator rather
/// than past it. That is reproduced here as a saturating decrement
/// when the index reaches the line's length, and is load-bearing for
/// [`is_pos_in_string`]'s own loop termination.
#[must_use]
pub fn skip_string(line: &[u8], p: usize) -> usize {
    let mut p = p;
    let at = |i: usize| line.get(i).copied().unwrap_or(0);

    // We loop, because strings may be concatenated: "date""time".
    loop {
        if at(p) == b'\'' {
            // 'c' or '\n' or '\000'
            if at(p + 1) == 0 {
                // ' at end of line
                break;
            }
            let mut i = 2usize;
            if at(p + 1) == b'\\' && at(p + 2) != 0 {
                // '\n' or '\000'
                i += 1;
                while crate::ascii_defs::ascii_isdigit(i32::from(at(p + i - 1))) {
                    i += 1;
                }
            }
            if at(p + i - 1) != 0 && at(p + i) == b'\'' {
                // check for trailing '
                p += i;
                p += 1;
                continue;
            }
        } else if at(p) == b'"' {
            // start of string
            p += 1;
            while at(p) != 0 {
                if at(p) == b'\\' && at(p + 1) != 0 {
                    p += 1;
                } else if at(p) == b'"' {
                    // end of string
                    break;
                }
                p += 1;
            }
            if at(p) == b'"' {
                p += 1;
                continue; // continue for another string
            }
        } else if at(p) == b'R' && at(p + 1) == b'"' {
            // Raw string: R"[delim](...)[delim]"
            let delim = p + 2;
            if let Some(rel) = crate::strings::vim_strchr(
                line.get(delim.min(line.len())..).unwrap_or(&[]),
                i32::from(b'('),
            ) {
                let delim_len = rel;
                p += 3;
                while at(p) != 0 {
                    if at(p) == b')'
                        && line
                            .get(p + 1..(p + 1 + delim_len).min(line.len()))
                            .is_some_and(|s| {
                                s.len() == delim_len
                                    && s == &line[delim..delim + delim_len]
                            })
                        && at(p + delim_len + 1) == b'"'
                    {
                        p += delim_len + 1;
                        break;
                    }
                    p += 1;
                }
                if at(p) == b'"' {
                    p += 1;
                    continue; // continue for another string
                }
            }
        }
        break; // no string found
    }

    if at(p) == 0 {
        // backup from NUL
        p = p.saturating_sub(1);
    }
    p
}

/// Whether `line[col]` is inside a C string (`is_pos_in_string`).
#[must_use]
pub fn is_pos_in_string(line: &[u8], col: usize) -> bool {
    let mut p = 0usize;
    while line.get(p).is_some_and(|&c| c != 0) && p < col {
        p = skip_string(line, p);
        p += 1;
    }
    p > col
}

/// Skip over white space and C comments within the line
/// (`cin_skipcomment`). Also skips Perl/shell `#` comments when the
/// buffer's own `b_ind_hash_comment` is set.
///
/// Returns the byte index just past whatever was skipped. Like the
/// original, a `//` or `#` comment consumes the rest of the line, and
/// an UNTERMINATED `/*` comment does too.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`, matching [`cindent_on`]'s own safety doc.
#[must_use]
pub unsafe fn cin_skipcomment(line: &[u8], s: usize) -> usize {
    // SAFETY: forwarded from this function's own safety doc.
    let hash_comment = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf }.b_ind_hash_comment;
    let at = |i: usize| line.get(i).copied().unwrap_or(0);
    let mut s = s;

    while at(s) != 0 {
        let prev_s = s;

        s += crate::charset::skipwhite(line.get(s..).unwrap_or(&[]));

        // Perl/shell # comment continues until eol. Require a space
        // before # to avoid recognizing $#array.
        if hash_comment != 0 && s != prev_s && at(s) == b'#' {
            return line.len();
        }
        if at(s) != b'/' {
            break;
        }
        s += 1;
        if at(s) == b'/' {
            // slash-slash comment continues till eol
            return line.len();
        }
        if at(s) != b'*' {
            break;
        }
        // skip slash-star comment
        s += 1;
        while at(s) != 0 {
            if at(s) == b'*' && at(s + 1) == b'/' {
                s += 2;
                break;
            }
            s += 1;
        }
    }
    s
}

/// Whether there is no code at `s` (`cin_nocode`). White space and
/// comments are not considered code.
///
/// # Safety
/// Forwarded from [`cin_skipcomment`]'s own safety doc.
#[must_use]
pub unsafe fn cin_nocode(line: &[u8], s: usize) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let end = unsafe { cin_skipcomment(line, s) };
    line.get(end).copied().unwrap_or(0) == 0
}

/// Whether `line[s..]` matches `"label:"`, returning the index just
/// past the `':'` when it does (`cin_islabel_skip`).
///
/// The original takes `const char **s` and advances it in place,
/// returning a bool; returning `Option<usize>` says the same thing
/// without the pointer-to-pointer - `None` is the original's `false`
/// (with `*s` left wherever it stopped, which no caller reads on that
/// path).
///
/// `"::"` is C++ scope resolution, not a label, so it is rejected.
///
/// # Safety
/// Forwarded from [`cin_skipcomment`]'s own safety doc.
#[must_use]
pub unsafe fn cin_islabel_skip(line: &[u8], s: usize) -> Option<usize> {
    let at = |i: usize| line.get(i).copied().unwrap_or(0);
    let mut s = s;

    // need at least one ID character
    if !crate::charset::vim_isidc(i32::from(at(s))) {
        return None;
    }
    while crate::charset::vim_isidc(i32::from(at(s))) {
        // SAFETY: `utfc_ptr2len` only reads the slice it is given.
        let adv = unsafe { crate::mbyte::utfc_ptr2len(line.get(s..).unwrap_or(&[])) };
        s += usize::try_from(adv).unwrap_or(1).max(1);
    }

    // SAFETY: forwarded from this function's own safety doc.
    s = unsafe { cin_skipcomment(line, s) };

    // "::" is not a label, it's C++
    if at(s) == b':' && at(s + 1) != b':' {
        Some(s + 1)
    } else {
        None
    }
}

/// Whether `line` starts with `"key:"` (`cin_has_js_key`) - the
/// JavaScript object-literal key form, optionally quoted as `'key':`
/// or `"key":`.
///
/// # Safety
/// Forwarded from [`cin_skipcomment`]'s own safety doc.
#[must_use]
pub unsafe fn cin_has_js_key(line: &[u8]) -> bool {
    let at = |i: usize| line.get(i).copied().unwrap_or(0);
    let mut s = crate::charset::skipwhite(line);

    let mut quote = 0u8;
    if at(s) == b'\'' || at(s) == b'"' {
        // can be 'key': or "key":
        quote = at(s);
        s += 1;
    }
    // need at least one ID character
    if !crate::charset::vim_isidc(i32::from(at(s))) {
        return false;
    }
    while crate::charset::vim_isidc(i32::from(at(s))) {
        s += 1;
    }
    // Note the original's own `*s && *s == quote` - when no quote was
    // seen `quote` is NUL, and the leading `*s` test is what stops a
    // line ending here from matching it.
    if at(s) != 0 && at(s) == quote {
        s += 1;
    }

    // SAFETY: forwarded from this function's own safety doc.
    s = unsafe { cin_skipcomment(line, s) };

    // "::" is not a label, it's C++
    at(s) == b':' && at(s + 1) != b':'
}

/// Recognize a `"default"` switch label (`cin_isdefault`).
///
/// # Safety
/// Forwarded from [`cin_skipcomment`]'s own safety doc.
#[must_use]
pub unsafe fn cin_isdefault(line: &[u8], s: usize) -> bool {
    if !line.get(s..).is_some_and(|t| t.starts_with(b"default")) {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let skip = unsafe { cin_skipcomment(line, s + 7) };
    let at = |i: usize| line.get(i).copied().unwrap_or(0);
    at(skip) == b':' && at(skip + 1) != b':'
}

/// Recognize a switch label - `"case .*:"` or `"default:"`
/// (`cin_iscase`).
///
/// `strict` false relaxes the check for JavaScript, where a string
/// after `case` still counts as a label.
///
/// # Safety
/// Forwarded from [`cin_skipcomment`]'s own safety doc.
#[must_use]
pub unsafe fn cin_iscase(line: &[u8], s: usize, strict: bool) -> bool {
    let at = |i: usize| line.get(i).copied().unwrap_or(0);
    // SAFETY: forwarded from this function's own safety doc.
    let s = unsafe { cin_skipcomment(line, s) };

    if cin_starts_with(line.get(s..).unwrap_or(&[]), b"case") {
        let mut p = s + 4;
        while at(p) != 0 {
            // SAFETY: forwarded from this function's own safety doc.
            p = unsafe { cin_skipcomment(line, p) };
            if at(p) == 0 {
                break;
            }
            if at(p) == b':' {
                if at(p + 1) == b':' {
                    // skip over "::" for C++
                    p += 1;
                } else {
                    return true;
                }
            }
            if at(p) == b'\'' && at(p + 1) != 0 && at(p + 2) == b'\'' {
                p += 2; // skip over ':'
            } else if at(p) == b'/' && (at(p + 1) == b'*' || at(p + 1) == b'/') {
                return false; // stop at comment
            } else if at(p) == b'"' {
                // JS etc.
                return !strict; // strict: stop at string
            }
            p += 1;
        }
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { cin_isdefault(line, s) }
}

/// Recognize a scope declaration label from the `'cinscopedecls'`
/// option (`cin_isscopedecl`).
///
/// # Safety
/// Forwarded from [`cin_skipcomment`]'s own safety doc.
#[must_use]
pub unsafe fn cin_isscopedecl(line: &[u8], p: usize) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let s = unsafe { cin_skipcomment(line, p) };
    // SAFETY: forwarded from this function's own safety doc.
    let cinsd = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf }
        .b_p_cinsd
        .clone()
        .unwrap_or_default();
    let cinsd_len = cinsd.len() + 1;
    let at = |i: usize| line.get(i).copied().unwrap_or(0);

    let mut c = 0usize;
    while c < cinsd.len() {
        let (part, next) = crate::option::copy_option_part(&cinsd, c, cinsd_len, b",");
        c = next;
        if line.get(s..).is_some_and(|t| t.starts_with(&part)) {
            // SAFETY: forwarded from this function's own safety doc.
            let skip = unsafe { cin_skipcomment(line, s + part.len()) };
            if at(skip) == b':' && at(skip + 1) != b':' {
                return true;
            }
        }
    }
    false
}

/// Skip comments and strings repeatedly until neither applies
/// (`cin_skip_comment_and_string`).
///
/// The original loops until the position stops moving, because
/// skipping a comment can expose a string and vice versa.
///
/// # Safety
/// Forwarded from [`cin_skipcomment`]'s own safety doc.
#[must_use]
pub unsafe fn cin_skip_comment_and_string(line: &[u8], s: usize) -> usize {
    let mut p = s;
    loop {
        let r = p;
        // SAFETY: forwarded from this function's own safety doc.
        p = unsafe { cin_skipcomment(line, p) };
        if line.get(p).copied().unwrap_or(0) != 0 {
            p = skip_string(line, p);
        }
        if p == r {
            return p;
        }
    }
}

/// Recognize structure or compound literal initialization
/// (`cin_is_compound_init`): `=|return [&][(typecast)] [{]`, with an
/// arbitrary number of opening braces.
///
/// # Safety
/// Forwarded from [`cin_skipcomment`]'s own safety doc.
#[must_use]
pub unsafe fn cin_is_compound_init(line: &[u8], s: usize) -> bool {
    let at = |i: usize| line.get(i).copied().unwrap_or(0);
    let mut p = s;
    let mut r: Option<usize> = None;

    while at(p) != 0 {
        if at(p) == b'=' {
            // SAFETY: forwarded from this function's own safety doc.
            p = unsafe { cin_skipcomment(line, p + 1) };
            r = Some(p);
        } else if line.get(p..).is_some_and(|t| t.starts_with(b"return"))
            && !crate::charset::vim_isidc(i32::from(at(p + 6)))
            && (p == s || !crate::charset::vim_isidc(i32::from(at(p - 1))))
        {
            // SAFETY: forwarded from this function's own safety doc.
            p = unsafe { cin_skipcomment(line, p + 6) };
            r = Some(p);
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            p = unsafe { cin_skip_comment_and_string(line, p + 1) };
        }
    }

    // p points now after '=' or "return"
    let Some(mut p) = r else {
        return false;
    };

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { cin_nocode(line, p) } {
        return true;
    }

    if at(p) == b'&' {
        // SAFETY: forwarded from this function's own safety doc.
        p = unsafe { cin_skipcomment(line, p + 1) };
    }

    if at(p) == b'(' {
        // skip a typecast
        let mut open_count: i32 = 1;
        loop {
            // SAFETY: forwarded from this function's own safety doc.
            p = unsafe { cin_skip_comment_and_string(line, p + 1) };
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { cin_nocode(line, p) } {
                return true;
            }
            open_count += i32::from(at(p) == b'(') - i32::from(at(p) == b')');
            if open_count == 0 {
                break;
            }
        }
        // SAFETY: forwarded from this function's own safety doc.
        p = unsafe { cin_skipcomment(line, p + 1) };
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { cin_nocode(line, p) } {
            return true;
        }
    }

    while at(p) == b'{' {
        // SAFETY: forwarded from this function's own safety doc.
        p = unsafe { cin_skipcomment(line, p + 1) };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { cin_nocode(line, p) }
}

/// Recognize a preprocessor statement: any line starting with `'#'`
/// (`cin_ispreproc`).
#[must_use]
pub fn cin_ispreproc(line: &[u8], s: usize) -> bool {
    let rest = line.get(s..).unwrap_or(&[]);
    let w = crate::charset::skipwhite(rest);
    rest.get(w).copied().unwrap_or(0) == b'#'
}

/// Recognize the start of a C or C++ comment (`cin_iscomment`).
#[must_use]
pub fn cin_iscomment(line: &[u8], p: usize) -> bool {
    let at = |i: usize| line.get(i).copied().unwrap_or(0);
    at(p) == b'/' && (at(p + 1) == b'*' || at(p + 1) == b'/')
}

/// Recognize the start of a `"//"` comment (`cin_islinecomment`).
#[must_use]
pub fn cin_islinecomment(line: &[u8], p: usize) -> bool {
    let at = |i: usize| line.get(i).copied().unwrap_or(0);
    at(p) == b'/' && at(p + 1) == b'/'
}

/// Recognize an `"if"` keyword (`cin_isif`).
///
/// Spelled out separately in the original even though it is exactly
/// [`cin_starts_with`]`(p, "if")`; kept as its own function so the
/// call sites read the same as the original's.
#[must_use]
pub fn cin_isif(line: &[u8], p: usize) -> bool {
    cin_starts_with(line.get(p..).unwrap_or(&[]), b"if")
}

/// Recognize a `"do"` keyword (`cin_isdo`).
#[must_use]
pub fn cin_isdo(line: &[u8], p: usize) -> bool {
    cin_starts_with(line.get(p..).unwrap_or(&[]), b"do")
}

/// Recognize a `"break"` keyword (`cin_isbreak`).
#[must_use]
pub fn cin_isbreak(line: &[u8], p: usize) -> bool {
    cin_starts_with(line.get(p..).unwrap_or(&[]), b"break")
}

/// Recognize an `"else"` keyword, also accepting `"} else"`
/// (`cin_iselse`).
///
/// # Safety
/// Forwarded from [`cin_skipcomment`]'s own safety doc.
#[must_use]
pub unsafe fn cin_iselse(line: &[u8], p: usize) -> bool {
    let mut p = p;
    if line.get(p).copied().unwrap_or(0) == b'}' {
        // accept "} else"
        // SAFETY: forwarded from this function's own safety doc.
        p = unsafe { cin_skipcomment(line, p + 1) };
    }
    cin_starts_with(line.get(p..).unwrap_or(&[]), b"else")
}

/// Whether `line[s..]` ends in `find`, ignoring trailing white space
/// and comments (`cin_ends_in`). Strings and comments are skipped
/// while scanning.
///
/// # Safety
/// Forwarded from [`cin_skipcomment`]'s own safety doc.
#[must_use]
pub unsafe fn cin_ends_in(line: &[u8], s: usize, find: &[u8]) -> bool {
    let at = |i: usize| line.get(i).copied().unwrap_or(0);
    let mut p = s;

    while at(p) != 0 {
        // SAFETY: forwarded from this function's own safety doc.
        p = unsafe { cin_skipcomment(line, p) };
        if line.get(p..).is_some_and(|t| t.starts_with(find)) {
            let after = p + find.len();
            let r = after + crate::charset::skipwhite(line.get(after..).unwrap_or(&[]));
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { cin_nocode(line, r) } {
                return true;
            }
        }
        if at(p) != 0 {
            p += 1;
        }
    }
    false
}

/// Recognize an `extern "C"` or `extern "C++"` linkage specification
/// (`cin_is_cpp_extern_c`).
///
/// Requires exactly one string literal: a second one (as in
/// `extern "C" "C"`) is rejected, as is anything else between the
/// keyword and an opening brace.
///
/// # Safety
/// Forwarded from [`cin_skipcomment`]'s own safety doc.
#[must_use]
pub unsafe fn cin_is_cpp_extern_c(line: &[u8], s: usize) -> bool {
    let at = |i: usize| line.get(i).copied().unwrap_or(0);
    // SAFETY: forwarded from this function's own safety doc.
    let s = unsafe { cin_skipcomment(line, s) };

    // Note the keyword test here uses `vim_iswordc`, not the
    // `vim_isIDc` that `cin_starts_with` uses - so this is
    // deliberately NOT expressed via that helper.
    if !line.get(s..).is_some_and(|t| t.starts_with(b"extern")) {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc - the
    // same `curbuf` read.
    let boundary_ok =
        at(s + 6) == 0 || !unsafe { crate::charset::vim_iswordc(i32::from(at(s + 6))) };
    if !boundary_ok {
        return false;
    }

    let after = s + 6;
    let w = after + crate::charset::skipwhite(line.get(after..).unwrap_or(&[]));
    // SAFETY: forwarded from this function's own safety doc.
    let mut p = unsafe { cin_skipcomment(line, w) };

    let mut has_string_literal = false;
    while at(p) != 0 {
        if crate::ascii_defs::ascii_iswhite(i32::from(at(p))) {
            let w = p + crate::charset::skipwhite(line.get(p..).unwrap_or(&[]));
            // SAFETY: forwarded from this function's own safety doc.
            p = unsafe { cin_skipcomment(line, w) };
        } else if at(p) == b'{' {
            break;
        } else if at(p) == b'"' && at(p + 1) == b'C' && at(p + 2) == b'"' {
            if has_string_literal {
                return false;
            }
            has_string_literal = true;
            p += 3;
        } else if at(p) == b'"'
            && at(p + 1) == b'C'
            && at(p + 2) == b'+'
            && at(p + 3) == b'+'
            && at(p + 4) == b'"'
        {
            if has_string_literal {
                return false;
            }
            has_string_literal = true;
            p += 5;
        } else {
            return false;
        }
    }
    has_string_literal
}

/// Recognize a line that starts with `'{'` or `'}'`, or ends with
/// `';'`, `','`, `'{'` or `'}'` (`cin_isterminated`).
///
/// `"} else"` is not considered a terminated line. If a line begins
/// with an `"else"`, it is only terminated when no unmatched opening
/// braces follow - which handles `"else { foo();"` correctly.
///
/// `incl_open` includes a trailing `'{'` as a terminator;
/// `incl_comma` recognizes a trailing comma.
///
/// Returns the character terminating the line, or `0` for none.
/// Ending characters take precedence over the leading-brace case,
/// which is only returned if the scan finds nothing better.
///
/// # Safety
/// Forwarded from [`cin_skipcomment`]'s own safety doc.
#[must_use]
pub unsafe fn cin_isterminated(
    line: &[u8],
    s: usize,
    incl_open: bool,
    incl_comma: bool,
) -> u8 {
    let at = |i: usize| line.get(i).copied().unwrap_or(0);
    let mut found_start = 0u8;
    let mut n_open = 0u32;
    let mut is_else = false;

    // SAFETY: forwarded from this function's own safety doc.
    let mut s = unsafe { cin_skipcomment(line, s) };

    // SAFETY: forwarded from this function's own safety doc.
    if at(s) == b'{' || (at(s) == b'}' && !unsafe { cin_iselse(line, s) }) {
        found_start = at(s);
    }

    if found_start == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        is_else = unsafe { cin_iselse(line, s) };
    }

    while at(s) != 0 {
        // skip over comments, "" strings and 'c'haracters
        // SAFETY: forwarded from this function's own safety doc.
        s = skip_string(line, unsafe { cin_skipcomment(line, s) });
        if at(s) == b'}' && n_open > 0 {
            n_open -= 1;
        }
        // SAFETY: forwarded from this function's own safety doc.
        if (!is_else || n_open == 0)
            && (at(s) == b';' || at(s) == b'}' || (incl_comma && at(s) == b','))
            && unsafe { cin_nocode(line, s + 1) }
        {
            return at(s);
        } else if at(s) == b'{' {
            // SAFETY: forwarded from this function's own safety doc.
            if incl_open && unsafe { cin_nocode(line, s + 1) } {
                return at(s);
            }
            n_open += 1;
        }

        if at(s) != 0 {
            s += 1;
        }
    }
    found_start
}

/// Recognize a `"namespace"` scope declaration
/// (`cin_is_cpp_namespace`).
///
/// Accepts an optional `"inline"`/`"export"` prefix in any order, an
/// optional namespace name, and C++17 nested namespace names joined
/// by `"::"`.
///
/// # Safety
/// Forwarded from [`cin_skipcomment`]'s own safety doc.
#[must_use]
pub unsafe fn cin_is_cpp_namespace(line: &[u8], s: usize) -> bool {
    let at = |i: usize| line.get(i).copied().unwrap_or(0);
    // SAFETY: forwarded from this function's own safety doc - the
    // same `curbuf` read `vim_iswordc` performs.
    let isword = |c: u8| unsafe { crate::charset::vim_iswordc(i32::from(c)) };
    let starts = |i: usize, w: &[u8]| line.get(i..).is_some_and(|t| t.starts_with(w));

    // SAFETY: forwarded from this function's own safety doc.
    let mut s = unsafe { cin_skipcomment(line, s) };

    // skip over "inline" and "export" in any order
    while (starts(s, b"inline") || starts(s, b"export")) && (at(s + 6) == 0 || !isword(at(s + 6)))
    {
        let after = s + 6;
        let w = after + crate::charset::skipwhite(line.get(after..).unwrap_or(&[]));
        // SAFETY: forwarded from this function's own safety doc.
        s = unsafe { cin_skipcomment(line, w) };
    }

    if !(starts(s, b"namespace") && (at(s + 9) == 0 || !isword(at(s + 9)))) {
        return false;
    }

    let after = s + 9;
    let w = after + crate::charset::skipwhite(line.get(after..).unwrap_or(&[]));
    // SAFETY: forwarded from this function's own safety doc.
    let mut p = unsafe { cin_skipcomment(line, w) };

    let mut has_name = false;
    let mut has_name_start = false;
    while at(p) != 0 {
        if crate::ascii_defs::ascii_iswhite(i32::from(at(p))) {
            has_name = true; // found end of a name
            let w = p + crate::charset::skipwhite(line.get(p..).unwrap_or(&[]));
            // SAFETY: forwarded from this function's own safety doc.
            p = unsafe { cin_skipcomment(line, w) };
        } else if at(p) == b'{' {
            break;
        } else if isword(at(p)) {
            has_name_start = true;
            if has_name {
                return false; // word character after skipping past name
            }
            p += 1;
        } else if at(p) == b':' && at(p + 1) == b':' && isword(at(p + 2)) {
            if !has_name_start || has_name {
                return false;
            }
            // C++ 17 nested namespace
            p += 3;
        } else {
            return false;
        }
    }
    true
}

/// The index of the first non-empty non-comment character after a
/// `':'` (`after_label`), or `None` when there is none.
///
/// For `"case 234:    a = b;"` this points at the `a`.
///
/// # Safety
/// Forwarded from [`cin_skipcomment`]'s own safety doc.
#[must_use]
pub unsafe fn after_label(line: &[u8], l: usize) -> Option<usize> {
    let at = |i: usize| line.get(i).copied().unwrap_or(0);
    let mut l = l;

    while at(l) != 0 {
        if at(l) == b':' {
            if at(l + 1) == b':' {
                // skip over "::" for C++
                l += 1;
            } else {
                // SAFETY: forwarded from this function's own safety doc.
                if !unsafe { cin_iscase(line, l + 1, false) } {
                    break;
                }
            }
        } else if at(l) == b'\'' && at(l + 1) != 0 && at(l + 2) == b'\'' {
            l += 2; // skip over 'x'
        }
        l += 1;
    }
    if at(l) == 0 {
        return None;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let l = unsafe { cin_skipcomment(line, l + 1) };
    if at(l) == 0 { None } else { Some(l) }
}

/// Whether an `"if"`, `"for"` or `"while"` keyword sits just before
/// `offset` (`cin_is_if_for_while_before_offset`).
///
/// The original takes `int *poffset` as an in/out parameter; here
/// `offset` is passed by value and the updated position is returned
/// as `Some(new_offset)`, with `None` for "not found".
///
/// Note the whitespace-skipping loop's own `offset > 2` bound is
/// STRICT, so it stops early when the keyword sits at the very start
/// of the line: `"if ("` (keyword at column 0, one space) is NOT
/// recognized, while the same text indented (`"  if ("`) is. That
/// looks like an off-by-one in the original, but it is preserved
/// rather than "fixed", and pinned by its own test.
#[must_use]
pub fn cin_is_if_for_while_before_offset(line: &[u8], offset: usize) -> Option<usize> {
    let at = |i: usize| line.get(i).copied().unwrap_or(0);

    // `if (offset-- < 2)` - compares first, then decrements.
    if offset < 2 {
        return None;
    }
    let mut offset = offset - 1;

    while offset > 2 && crate::ascii_defs::ascii_iswhite(i32::from(at(offset))) {
        offset -= 1;
    }

    let starts = |i: usize, w: &[u8]| line.get(i..).is_some_and(|t| t.starts_with(w));

    offset -= 1;
    let found = if starts(offset, b"if") {
        true
    } else if offset >= 1 {
        offset -= 1;
        if starts(offset, b"for") {
            true
        } else if offset >= 2 {
            offset -= 2;
            starts(offset, b"while")
        } else {
            false
        }
    } else {
        false
    };

    if !found {
        return None;
    }
    // The keyword must not itself be the tail of a longer identifier.
    if offset == 0 || !crate::charset::vim_isidc(i32::from(at(offset - 1))) {
        Some(offset)
    } else {
        None
    }
}

/// The column of a line comment's start, or `MAXCOL` when there is
/// none (`check_linecomment`).
///
/// The line is scanned once (skipping strings), so this stays linear
/// even on lines with many slashes.
///
/// For a `'lisp'` buffer this looks for a `';'` comment instead,
/// tracking string state as it goes.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`, matching [`cindent_on`]'s own safety doc.
#[must_use]
pub unsafe fn check_linecomment(line: &[u8]) -> i32 {
    let at = |i: usize| line.get(i).copied().unwrap_or(0);
    // SAFETY: forwarded from this function's own safety doc.
    let lisp = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf }.b_p_lisp != 0;

    let mut found: Option<usize> = None;

    if lisp {
        // skip Lispish one-line comments
        if crate::strings::vim_strchr(line, i32::from(b';')).is_some() {
            // there may be comments
            let mut in_str = false; // inside of string
            let mut p = 0usize;
            while let Some(rel) = line
                .get(p..)
                .and_then(|t| t.iter().position(|&c| c == b'"' || c == b';'))
            {
                p += rel;
                if at(p) == b'"' {
                    if in_str {
                        if at(p.wrapping_sub(1)) != b'\\' {
                            // skip escaped quote
                            in_str = false;
                        }
                    } else if p == 0
                        || (p >= 2
                            // skip #\" form
                            && at(p - 1) != b'\\'
                            && at(p - 2) != b'#')
                    {
                        in_str = true;
                    }
                } else if !in_str
                    && (p < 2 || (at(p - 1) != b'\\' && at(p - 2) != b'#'))
                    && !is_pos_in_string(line, p)
                {
                    found = Some(p); // found!
                    break;
                }
                p += 1;
            }
        }
    } else {
        // Scan the line once, skipping over strings, char constants
        // and raw strings, instead of testing each '/' with
        // is_pos_in_string() (which rescans from the start, making
        // this quadratic on lines with many slashes).
        let mut p = 0usize;
        while at(p) != 0 {
            p = skip_string(line, p);
            if at(p) == 0 {
                break;
            }
            // Accept a double /, unless it's preceded with * and
            // followed by *, because * / / * is an end and start of a
            // C comment.
            if at(p) == b'/'
                && at(p + 1) == b'/'
                && (p == 0 || at(p - 1) != b'*' || at(p + 2) != b'*')
            {
                found = Some(p);
                break;
            }
            p += 1;
        }
    }

    match found {
        None => crate::pos_defs::MAXCOL,
        Some(p) => i32::try_from(p).unwrap_or(crate::pos_defs::MAXCOL),
    }
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

    /// Runs `cin_is_cinword` with `cinw` installed as the buffer's own
    /// `'cinwords'`.
    fn cinword_with(cinw: &[u8], line: &[u8]) -> bool {
        let mut buf = BufT {
            b_p_cinw: Some(cinw.to_vec()),
            ..Default::default()
        };
        let _guard = CurbufGuard::set(&mut buf);
        unsafe { cin_is_cinword(line) }
    }

    /// The real default `'cinwords'`, read off a live nvim binary.
    const DEFAULT_CINW: &[u8] = b"if,else,while,do,for,switch";

    // ---- skip_string / is_pos_in_string ----

    // ---- cin_skipcomment / cin_nocode ----

    /// Runs `f` with a buffer whose `b_ind_hash_comment` is `hash`.
    fn with_hash_comment<R>(hash: i32, f: impl FnOnce() -> R) -> R {
        let mut buf = BufT {
            b_ind_hash_comment: hash,
            ..Default::default()
        };
        let _guard = CurbufGuard::set(&mut buf);
        f()
    }

    // ---- cin_islabel_skip / cin_has_js_key ----

    // ---- cin_iscase / cin_isdefault / cin_isscopedecl ----

    /// Runs `f` with a buffer whose `'cinscopedecls'` is `cinsd`.
    fn with_cinsd<R>(cinsd: &[u8], f: impl FnOnce() -> R) -> R {
        let mut buf = BufT {
            b_p_cinsd: Some(cinsd.to_vec()),
            ..Default::default()
        };
        let _guard = CurbufGuard::set(&mut buf);
        f()
    }

    /// The real default `'cinscopedecls'`, read off a live nvim binary.
    const DEFAULT_CINSD: &[u8] = b"public,protected,private";

    // ---- cin_skip_comment_and_string / cin_is_compound_init ----

    // ---- small cin_* predicates ----

    // ---- cin_is_cpp_extern_c ----

    // ---- cin_isterminated ----

    // ---- cin_is_cpp_namespace / after_label ----

    // ---- cin_is_if_for_while_before_offset ----

    // ---- check_linecomment ----

    /// Runs `f` with a buffer whose `'lisp'` option is `lisp`.
    fn with_lisp<R>(lisp: i32, f: impl FnOnce() -> R) -> R {
        let mut buf = BufT {
            b_p_lisp: lisp,
            ..Default::default()
        };
        let _guard = CurbufGuard::set(&mut buf);
        f()
    }

    #[test]
    fn check_linecomment_finds_a_slash_slash_comment() {
        with_lisp(0, || {
            assert_eq!(unsafe { check_linecomment(b"// c") }, 0);
            assert_eq!(unsafe { check_linecomment(b"x = 1; // c") }, 7);
        });
    }

    #[test]
    fn check_linecomment_is_maxcol_when_there_is_no_comment() {
        with_lisp(0, || {
            assert_eq!(
                unsafe { check_linecomment(b"x = 1;") },
                crate::pos_defs::MAXCOL
            );
            assert_eq!(unsafe { check_linecomment(b"") }, crate::pos_defs::MAXCOL);
            // A single slash is division, not a comment.
            assert_eq!(
                unsafe { check_linecomment(b"a / b") },
                crate::pos_defs::MAXCOL
            );
        });
    }

    #[test]
    fn check_linecomment_ignores_slashes_inside_a_string() {
        with_lisp(0, || {
            // The `//` inside the literal must not count...
            assert_eq!(
                unsafe { check_linecomment(b"x = \"http://a\";") },
                crate::pos_defs::MAXCOL
            );
            // ...but a real one after it does.
            assert_eq!(unsafe { check_linecomment(b"x = \"a\"; // c") }, 9);
        });
    }

    #[test]
    fn check_linecomment_rejects_the_star_slash_slash_star_sequence() {
        with_lisp(0, || {
            // `*//*` is the end of one C comment and the start of the
            // next, not a line comment, so it is skipped.
            assert_eq!(
                unsafe { check_linecomment(b"a*//*b") },
                crate::pos_defs::MAXCOL
            );
        });
    }

    #[test]
    fn check_linecomment_finds_a_semicolon_comment_in_lisp_mode() {
        with_lisp(1, || {
            assert_eq!(unsafe { check_linecomment(b"; c") }, 0);
            assert_eq!(unsafe { check_linecomment(b"(foo) ; c") }, 6);
            // No semicolon at all.
            assert_eq!(
                unsafe { check_linecomment(b"(foo)") },
                crate::pos_defs::MAXCOL
            );
        });
    }

    #[test]
    fn check_linecomment_lisp_mode_ignores_a_semicolon_inside_a_string() {
        with_lisp(1, || {
            // The `;` inside the literal must not count...
            assert_eq!(
                unsafe { check_linecomment(b"(foo \";\")") },
                crate::pos_defs::MAXCOL
            );
            // ...but a real one after it does.
            assert_eq!(unsafe { check_linecomment(b"(foo \"a\") ; c") }, 10);
        });
    }

    #[test]
    fn cin_is_if_for_while_before_offset_finds_an_indented_keyword() {
        // "  if (" - the `(` is at index 5, and the keyword starts at
        // index 2, which is what comes back.
        assert_eq!(cin_is_if_for_while_before_offset(b"  if (", 5), Some(2));
        // "  for (" - `(` at 6, keyword at 2.
        assert_eq!(cin_is_if_for_while_before_offset(b"  for (", 6), Some(2));
        // "  while (" - `(` at 8, keyword at 2.
        assert_eq!(cin_is_if_for_while_before_offset(b"  while (", 8), Some(2));
    }

    #[test]
    fn cin_is_if_for_while_before_offset_finds_a_keyword_with_no_space() {
        // At column 0 with NO space the scan still reaches the word.
        assert_eq!(cin_is_if_for_while_before_offset(b"if(", 2), Some(0));
    }

    #[test]
    fn cin_is_if_for_while_before_offset_misses_a_column_zero_keyword_with_a_space() {
        // The whitespace loop's `offset > 2` bound is STRICT, so it
        // refuses to step back over the space at index 2 and the
        // keyword is missed. Preserved from the original rather than
        // "fixed" - the same text indented is found (see above).
        assert_eq!(cin_is_if_for_while_before_offset(b"if (", 3), None);
    }

    #[test]
    fn cin_is_if_for_while_before_offset_rejects_a_longer_identifier() {
        // "xif (" - the `if` here is the tail of `xif`, so the
        // preceding-character check rejects it.
        assert_eq!(cin_is_if_for_while_before_offset(b" xif (", 6), None);
    }

    #[test]
    fn cin_is_if_for_while_before_offset_rejects_short_offsets_and_non_keywords() {
        // An offset below 2 can never have a keyword before it.
        assert_eq!(cin_is_if_for_while_before_offset(b"if (", 0), None);
        assert_eq!(cin_is_if_for_while_before_offset(b"if (", 1), None);
        // No keyword at all.
        assert_eq!(cin_is_if_for_while_before_offset(b"  foo (", 6), None);
        assert_eq!(cin_is_if_for_while_before_offset(b"", 5), None);
    }

    #[test]
    fn cin_is_cpp_namespace_accepts_named_and_anonymous_forms() {
        with_hash_comment(0, || {
            assert!(unsafe { cin_is_cpp_namespace(b"namespace", 0) });
            assert!(unsafe { cin_is_cpp_namespace(b"namespace foo", 0) });
            assert!(unsafe { cin_is_cpp_namespace(b"namespace foo {", 0) });
            // Anonymous namespace.
            assert!(unsafe { cin_is_cpp_namespace(b"namespace {", 0) });
        });
    }

    #[test]
    fn cin_is_cpp_namespace_accepts_inline_and_export_prefixes() {
        with_hash_comment(0, || {
            assert!(unsafe { cin_is_cpp_namespace(b"inline namespace foo", 0) });
            assert!(unsafe { cin_is_cpp_namespace(b"export namespace foo", 0) });
            // Both, in either order.
            assert!(unsafe { cin_is_cpp_namespace(b"inline export namespace foo", 0) });
            assert!(unsafe { cin_is_cpp_namespace(b"export inline namespace foo", 0) });
        });
    }

    #[test]
    fn cin_is_cpp_namespace_accepts_a_cpp17_nested_name() {
        with_hash_comment(0, || {
            assert!(unsafe { cin_is_cpp_namespace(b"namespace a::b", 0) });
            assert!(unsafe { cin_is_cpp_namespace(b"namespace a::b::c {", 0) });
        });
    }

    #[test]
    fn cin_is_cpp_namespace_rejects_a_second_name_after_the_first() {
        with_hash_comment(0, || {
            // Once a name has ended, another word character is not
            // allowed - this is not a namespace declaration.
            assert!(!unsafe { cin_is_cpp_namespace(b"namespace foo bar", 0) });
            // A "::" after the name has already ended is rejected too.
            assert!(!unsafe { cin_is_cpp_namespace(b"namespace foo ::b", 0) });
        });
    }

    #[test]
    fn cin_is_cpp_namespace_rejects_non_namespaces() {
        with_hash_comment(0, || {
            assert!(!unsafe { cin_is_cpp_namespace(b"", 0) });
            assert!(!unsafe { cin_is_cpp_namespace(b"struct foo {", 0) });
            // "namespaces" is not the keyword.
            assert!(!unsafe { cin_is_cpp_namespace(b"namespaces foo", 0) });
            // Unexpected punctuation in the name position.
            assert!(!unsafe { cin_is_cpp_namespace(b"namespace foo;", 0) });
        });
    }

    #[test]
    fn after_label_points_past_a_label_colon() {
        with_hash_comment(0, || {
            // "case 234:    a = b;" - the `a` is at index 13.
            let line = b"case 234:    a = b;";
            assert_eq!(unsafe { after_label(line, 0) }, Some(13));
            // A plain label works the same way.
            let line = b"done: x";
            assert_eq!(unsafe { after_label(line, 0) }, Some(6));
        });
    }

    #[test]
    fn after_label_skips_a_comment_after_the_colon() {
        with_hash_comment(0, || {
            let line = b"done:/* c */x";
            assert_eq!(unsafe { after_label(line, 0) }, Some(12));
        });
    }

    #[test]
    fn after_label_is_none_without_a_label_or_trailing_code() {
        with_hash_comment(0, || {
            // No colon at all.
            assert_eq!(unsafe { after_label(b"x = 1;", 0) }, None);
            assert_eq!(unsafe { after_label(b"", 0) }, None);
            // A colon with nothing after it.
            assert_eq!(unsafe { after_label(b"done:", 0) }, None);
            // Only a comment after it.
            assert_eq!(unsafe { after_label(b"done:/* c */", 0) }, None);
        });
    }

    #[test]
    fn after_label_skips_cpp_scope_resolution_and_char_constants() {
        with_hash_comment(0, || {
            // "::" is stepped over, so the real label colon is found.
            let line = b"std::foo: x";
            assert_eq!(unsafe { after_label(line, 0) }, Some(10));
            // A quoted colon is skipped as a char constant.
            let line = b"x = ':'; done: y";
            assert_eq!(unsafe { after_label(line, 0) }, Some(15));
        });
    }

    #[test]
    fn cin_isterminated_reports_the_terminating_character() {
        with_hash_comment(0, || {
            assert_eq!(unsafe { cin_isterminated(b"x = 1;", 0, false, false) }, b';');
            assert_eq!(unsafe { cin_isterminated(b"foo()}", 0, false, false) }, b'}');
            // Trailing whitespace and comments are ignored.
            assert_eq!(
                unsafe { cin_isterminated(b"x = 1; /* c */", 0, false, false) },
                b';'
            );
        });
    }

    #[test]
    fn cin_isterminated_honours_the_incl_open_and_incl_comma_flags() {
        with_hash_comment(0, || {
            // A trailing `{` only counts when incl_open is set.
            assert_eq!(unsafe { cin_isterminated(b"if (x) {", 0, false, false) }, 0);
            assert_eq!(unsafe { cin_isterminated(b"if (x) {", 0, true, false) }, b'{');
            // A trailing comma only counts when incl_comma is set.
            assert_eq!(unsafe { cin_isterminated(b"foo,", 0, false, false) }, 0);
            assert_eq!(unsafe { cin_isterminated(b"foo,", 0, false, true) }, b',');
        });
    }

    #[test]
    fn cin_isterminated_returns_a_leading_brace_when_nothing_better_is_found() {
        with_hash_comment(0, || {
            // A line that is JUST an opening brace reports it via the
            // `found_start` fallback, even with incl_open off.
            assert_eq!(unsafe { cin_isterminated(b"{", 0, false, false) }, b'{');
            // A lone closing brace terminates outright.
            assert_eq!(unsafe { cin_isterminated(b"}", 0, false, false) }, b'}');
        });
    }

    #[test]
    fn cin_isterminated_does_not_treat_close_brace_else_as_terminated() {
        with_hash_comment(0, || {
            // "} else" is explicitly not a terminated line - the
            // leading `}` is suppressed by the cin_iselse check.
            assert_eq!(unsafe { cin_isterminated(b"} else", 0, false, false) }, 0);
        });
    }

    #[test]
    fn cin_isterminated_requires_balanced_braces_after_a_leading_else() {
        with_hash_comment(0, || {
            // "else { foo();" has an unmatched opening brace, so the
            // `;` does NOT terminate it.
            assert_eq!(
                unsafe { cin_isterminated(b"else { foo();", 0, false, false) },
                0
            );
            // Once the brace is matched the `;` counts again.
            assert_eq!(
                unsafe { cin_isterminated(b"else { foo(); }", 0, false, false) },
                b'}'
            );
        });
    }

    #[test]
    fn cin_isterminated_ignores_characters_inside_strings() {
        with_hash_comment(0, || {
            // The `;` inside the string literal must not count.
            assert_eq!(
                unsafe { cin_isterminated(b"x = \";\"", 0, false, false) },
                0
            );
            // The real one outside it does.
            assert_eq!(
                unsafe { cin_isterminated(b"x = \";\";", 0, false, false) },
                b';'
            );
        });
    }

    #[test]
    fn cin_isterminated_is_zero_for_an_unterminated_line() {
        with_hash_comment(0, || {
            assert_eq!(unsafe { cin_isterminated(b"x = 1", 0, false, false) }, 0);
            assert_eq!(unsafe { cin_isterminated(b"", 0, false, false) }, 0);
        });
    }

    #[test]
    fn cin_is_cpp_extern_c_accepts_both_linkage_forms() {
        with_hash_comment(0, || {
            assert!(unsafe { cin_is_cpp_extern_c(b"extern \"C\"", 0) });
            assert!(unsafe { cin_is_cpp_extern_c(b"extern \"C++\"", 0) });
            // A following brace ends the scan successfully.
            assert!(unsafe { cin_is_cpp_extern_c(b"extern \"C\" {", 0) });
            // Comments are skipped before and within.
            assert!(unsafe { cin_is_cpp_extern_c(b"/* c */extern \"C\"", 0) });
            assert!(unsafe { cin_is_cpp_extern_c(b"extern /* c */ \"C\"", 0) });
        });
    }

    #[test]
    fn cin_is_cpp_extern_c_requires_exactly_one_string_literal() {
        with_hash_comment(0, || {
            // No literal at all.
            assert!(!unsafe { cin_is_cpp_extern_c(b"extern", 0) });
            assert!(!unsafe { cin_is_cpp_extern_c(b"extern {", 0) });
            // Two literals are rejected.
            assert!(!unsafe { cin_is_cpp_extern_c(b"extern \"C\" \"C\"", 0) });
        });
    }

    #[test]
    fn cin_is_cpp_extern_c_rejects_other_content_and_partial_keywords() {
        with_hash_comment(0, || {
            // An ordinary extern declaration is not a linkage spec.
            assert!(!unsafe { cin_is_cpp_extern_c(b"extern int x;", 0) });
            // An unrecognized literal.
            assert!(!unsafe { cin_is_cpp_extern_c(b"extern \"Rust\"", 0) });
            // "externx" is not the keyword.
            assert!(!unsafe { cin_is_cpp_extern_c(b"externx \"C\"", 0) });
            assert!(!unsafe { cin_is_cpp_extern_c(b"", 0) });
        });
    }

    #[test]
    fn cin_ispreproc_matches_a_hash_after_optional_whitespace() {
        assert!(cin_ispreproc(b"#include <x>", 0));
        assert!(cin_ispreproc(b"   # define X", 0));
        assert!(cin_ispreproc(b"\t#if", 0));
        assert!(!cin_ispreproc(b"x = 1;", 0));
        assert!(!cin_ispreproc(b"", 0));
    }

    #[test]
    fn cin_iscomment_and_cin_islinecomment_match_their_openers() {
        // A block comment is a comment but not a LINE comment.
        assert!(cin_iscomment(b"/* c */", 0));
        assert!(!cin_islinecomment(b"/* c */", 0));
        // A `//` comment is both.
        assert!(cin_iscomment(b"// c", 0));
        assert!(cin_islinecomment(b"// c", 0));
        // A lone slash is neither.
        assert!(!cin_iscomment(b"/ x", 0));
        assert!(!cin_islinecomment(b"/ x", 0));
        assert!(!cin_iscomment(b"", 0));
    }

    #[test]
    fn cin_isif_isdo_isbreak_require_a_whole_keyword() {
        assert!(cin_isif(b"if (x)", 0));
        assert!(cin_isdo(b"do {", 0));
        assert!(cin_isbreak(b"break;", 0));
        // A longer identifier merely starting with the keyword does
        // not count.
        assert!(!cin_isif(b"ifx", 0));
        assert!(!cin_isdo(b"done", 0));
        assert!(!cin_isbreak(b"breaks", 0));
        assert!(!cin_isif(b"", 0));
    }

    #[test]
    fn cin_iselse_accepts_a_leading_close_brace() {
        with_hash_comment(0, || {
            assert!(unsafe { cin_iselse(b"else", 0) });
            // "} else" is the whole point of the leading-brace case.
            assert!(unsafe { cin_iselse(b"} else", 0) });
            // A comment may sit between the brace and the keyword.
            assert!(unsafe { cin_iselse(b"}/* c */else", 0) });
            // A longer identifier does not count.
            assert!(!unsafe { cin_iselse(b"elsewhere", 0) });
            assert!(!unsafe { cin_iselse(b"} x", 0) });
        });
    }

    #[test]
    fn cin_ends_in_matches_only_at_the_real_end_of_code() {
        with_hash_comment(0, || {
            assert!(unsafe { cin_ends_in(b"foo,", 0, b",") });
            // Trailing whitespace and comments are ignored.
            assert!(unsafe { cin_ends_in(b"foo,  ", 0, b",") });
            assert!(unsafe { cin_ends_in(b"foo, /* c */", 0, b",") });
            // A multi-character needle works too.
            assert!(unsafe { cin_ends_in(b"struct foo", 0, b"foo") });
        });
    }

    #[test]
    fn cin_ends_in_is_false_when_code_follows_the_match() {
        with_hash_comment(0, || {
            // The comma is not at the end of the code.
            assert!(!unsafe { cin_ends_in(b"foo, bar", 0, b",") });
            assert!(!unsafe { cin_ends_in(b"foo;", 0, b",") });
            assert!(!unsafe { cin_ends_in(b"", 0, b",") });
        });
    }

    #[test]
    fn cin_skip_comment_and_string_loops_until_nothing_moves() {
        with_hash_comment(0, || {
            // A comment followed by a string: one pass would leave
            // the string, so the loop must run twice.
            let line = b"/* c */\"str\"x";
            assert_eq!(
                unsafe { cin_skip_comment_and_string(line, 0) },
                line.len() - 1
            );
            // Nothing to skip leaves the index alone.
            assert_eq!(unsafe { cin_skip_comment_and_string(b"x", 0) }, 0);
        });
    }

    #[test]
    fn cin_is_compound_init_accepts_an_assignment_opening_a_brace() {
        with_hash_comment(0, || {
            assert!(unsafe { cin_is_compound_init(b"int x = {", 0) });
            // Several opening braces are allowed.
            assert!(unsafe { cin_is_compound_init(b"int x = {{", 0) });
            // A bare `=` at end of line also counts.
            assert!(unsafe { cin_is_compound_init(b"int x =", 0) });
        });
    }

    #[test]
    fn cin_is_compound_init_accepts_return_forms() {
        with_hash_comment(0, || {
            assert!(unsafe { cin_is_compound_init(b"return {", 0) });
            assert!(unsafe { cin_is_compound_init(b"return", 0) });
            // `return &{` - the address-of form.
            assert!(unsafe { cin_is_compound_init(b"return &{", 0) });
        });
    }

    #[test]
    fn cin_is_compound_init_skips_a_typecast() {
        with_hash_comment(0, || {
            assert!(unsafe { cin_is_compound_init(b"x = (struct foo){", 0) });
            assert!(unsafe { cin_is_compound_init(b"return (T){", 0) });
            // Nested parens in the cast are balanced correctly.
            assert!(unsafe { cin_is_compound_init(b"x = (a(b)){", 0) });
        });
    }

    #[test]
    fn cin_is_compound_init_rejects_lines_without_an_assignment_or_return() {
        with_hash_comment(0, || {
            assert!(!unsafe { cin_is_compound_init(b"int x;", 0) });
            assert!(!unsafe { cin_is_compound_init(b"", 0) });
            assert!(!unsafe { cin_is_compound_init(b"{", 0) });
        });
    }

    #[test]
    fn cin_is_compound_init_rejects_trailing_code_after_the_brace() {
        with_hash_comment(0, || {
            // Real code after the brace means this is not just an
            // opening of a compound initializer.
            assert!(!unsafe { cin_is_compound_init(b"x = { 1", 0) });
            assert!(!unsafe { cin_is_compound_init(b"x = y;", 0) });
        });
    }

    #[test]
    fn cin_is_compound_init_requires_return_to_be_its_own_word() {
        with_hash_comment(0, || {
            // "returns" and "myreturn" are not the `return` keyword,
            // so neither line has an assignment or return at all.
            assert!(!unsafe { cin_is_compound_init(b"returns {", 0) });
            assert!(!unsafe { cin_is_compound_init(b"myreturn {", 0) });
        });
    }

    #[test]
    fn cin_isdefault_recognizes_a_default_label() {
        with_hash_comment(0, || {
            assert!(unsafe { cin_isdefault(b"default:", 0) });
            // A comment may sit between the word and the colon.
            assert!(unsafe { cin_isdefault(b"default/* c */:", 0) });
            // "::" is C++ scope resolution, not a label.
            assert!(!unsafe { cin_isdefault(b"default::x", 0) });
            // No colon at all.
            assert!(!unsafe { cin_isdefault(b"default x", 0) });
            assert!(!unsafe { cin_isdefault(b"defaults:", 0) });
        });
    }

    #[test]
    fn cin_iscase_recognizes_case_and_default_labels() {
        with_hash_comment(0, || {
            assert!(unsafe { cin_iscase(b"case 1:", 0, true) });
            assert!(unsafe { cin_iscase(b"case FOO:", 0, true) });
            assert!(unsafe { cin_iscase(b"default:", 0, true) });
            // Leading whitespace and comments are skipped first.
            assert!(unsafe { cin_iscase(b"  case 1:", 0, true) });
            assert!(unsafe { cin_iscase(b"/* c */case 1:", 0, true) });
        });
    }

    #[test]
    fn cin_iscase_skips_a_cpp_scope_resolution_inside_the_value() {
        with_hash_comment(0, || {
            // "Foo::Bar" contains "::", which is stepped over rather
            // than treated as the label's own colon.
            assert!(unsafe { cin_iscase(b"case Foo::Bar:", 0, true) });
            // Without a real trailing colon it is not a label.
            assert!(!unsafe { cin_iscase(b"case Foo::Bar", 0, true) });
        });
    }

    #[test]
    fn cin_iscase_handles_a_colon_char_constant() {
        with_hash_comment(0, || {
            // `case ':':` - the quoted colon must not end the label,
            // but the real one after it does.
            assert!(unsafe { cin_iscase(b"case ':':", 0, true) });
        });
    }

    #[test]
    fn cin_iscase_strict_flag_only_changes_the_string_case() {
        with_hash_comment(0, || {
            // A string after `case` stops a strict (C) check but is
            // accepted by the relaxed (JS) one.
            let line = b"case \"foo\":";
            assert!(!unsafe { cin_iscase(line, 0, true) });
            assert!(unsafe { cin_iscase(line, 0, false) });
        });
    }

    #[test]
    fn cin_iscase_stops_at_a_comment_before_any_colon() {
        with_hash_comment(0, || {
            // A comment INSIDE the value aborts the scan.
            assert!(!unsafe { cin_iscase(b"case x /* c */", 0, true) });
        });
    }

    #[test]
    fn cin_iscase_rejects_non_labels() {
        with_hash_comment(0, || {
            assert!(!unsafe { cin_iscase(b"x = 1;", 0, true) });
            assert!(!unsafe { cin_iscase(b"", 0, true) });
            // "cases" is not "case" - cin_starts_with requires a
            // non-identifier character to follow.
            assert!(!unsafe { cin_iscase(b"cases:", 0, true) });
        });
    }

    #[test]
    fn cin_isscopedecl_matches_each_default_cinscopedecl() {
        with_cinsd(DEFAULT_CINSD, || {
            assert!(unsafe { cin_isscopedecl(b"public:", 0) });
            assert!(unsafe { cin_isscopedecl(b"protected:", 0) });
            assert!(unsafe { cin_isscopedecl(b"private:", 0) });
            // Leading whitespace is skipped first.
            assert!(unsafe { cin_isscopedecl(b"   public:", 0) });
            // A comment may sit between the word and the colon.
            assert!(unsafe { cin_isscopedecl(b"public/* c */:", 0) });
        });
    }

    #[test]
    fn cin_isscopedecl_rejects_non_matches() {
        with_cinsd(DEFAULT_CINSD, || {
            // No colon.
            assert!(!unsafe { cin_isscopedecl(b"public x", 0) });
            // "::" is scope resolution, not a declaration label.
            assert!(!unsafe { cin_isscopedecl(b"public::x", 0) });
            // Not in the list at all.
            assert!(!unsafe { cin_isscopedecl(b"internal:", 0) });
            assert!(!unsafe { cin_isscopedecl(b"", 0) });
        });
        // An empty 'cinscopedecls' can never match.
        with_cinsd(b"", || {
            assert!(!unsafe { cin_isscopedecl(b"public:", 0) });
        });
    }

    #[test]
    fn cin_islabel_skip_accepts_a_plain_label() {
        with_hash_comment(0, || {
            // "done:" - returns the index just past the ':'.
            assert_eq!(unsafe { cin_islabel_skip(b"done:", 0) }, Some(5));
            // Trailing code after the colon does not matter.
            assert_eq!(unsafe { cin_islabel_skip(b"lab: x = 1;", 0) }, Some(4));
        });
    }

    #[test]
    fn cin_islabel_skip_rejects_cpp_scope_resolution() {
        with_hash_comment(0, || {
            // "std::" is C++ scope resolution, not a label.
            assert_eq!(unsafe { cin_islabel_skip(b"std::cout", 0) }, None);
        });
    }

    #[test]
    fn cin_islabel_skip_requires_an_id_char_and_a_colon() {
        with_hash_comment(0, || {
            // No identifier at all.
            assert_eq!(unsafe { cin_islabel_skip(b":x", 0) }, None);
            assert_eq!(unsafe { cin_islabel_skip(b"", 0) }, None);
            // An identifier with no colon after it.
            assert_eq!(unsafe { cin_islabel_skip(b"done", 0) }, None);
            assert_eq!(unsafe { cin_islabel_skip(b"done = 1", 0) }, None);
        });
    }

    #[test]
    fn cin_islabel_skip_allows_a_comment_before_the_colon() {
        with_hash_comment(0, || {
            // The comment skipper runs between the identifier and the
            // colon, so this still reads as a label.
            let line = b"done/* c */:";
            assert_eq!(
                unsafe { cin_islabel_skip(line, 0) },
                Some(line.len())
            );
        });
    }

    #[test]
    fn cin_has_js_key_accepts_bare_and_quoted_keys() {
        with_hash_comment(0, || {
            assert!(unsafe { cin_has_js_key(b"key: 1") });
            assert!(unsafe { cin_has_js_key(b"'key': 1") });
            assert!(unsafe { cin_has_js_key(b"\"key\": 1") });
            // Leading whitespace is skipped first.
            assert!(unsafe { cin_has_js_key(b"   key: 1") });
        });
    }

    #[test]
    fn cin_has_js_key_rejects_non_keys() {
        with_hash_comment(0, || {
            // No colon.
            assert!(!unsafe { cin_has_js_key(b"key = 1") });
            // No identifier.
            assert!(!unsafe { cin_has_js_key(b": 1") });
            assert!(!unsafe { cin_has_js_key(b"") });
            // C++ scope resolution again.
            assert!(!unsafe { cin_has_js_key(b"std::cout") });
        });
    }

    #[test]
    fn cin_has_js_key_only_consumes_a_matching_closing_quote() {
        with_hash_comment(0, || {
            // Mismatched quotes: the `"` does not match the opening
            // `'`, so it is not consumed and the colon is never
            // reached.
            assert!(!unsafe { cin_has_js_key(b"'key\": 1") });
        });
    }

    #[test]
    fn cin_skipcomment_skips_leading_whitespace() {
        with_hash_comment(0, || {
            assert_eq!(unsafe { cin_skipcomment(b"   x", 0) }, 3);
            assert_eq!(unsafe { cin_skipcomment(b"\t\tx", 0) }, 2);
            // Nothing to skip.
            assert_eq!(unsafe { cin_skipcomment(b"x", 0) }, 0);
        });
    }

    #[test]
    fn cin_skipcomment_consumes_a_line_comment_to_end_of_line() {
        with_hash_comment(0, || {
            let line = b"// comment";
            assert_eq!(unsafe { cin_skipcomment(line, 0) }, line.len());
        });
    }

    #[test]
    fn cin_skipcomment_skips_a_block_comment_and_stops_after_it() {
        with_hash_comment(0, || {
            // The `x` follows the closing `*/`.
            let line = b"/* c */x";
            assert_eq!(unsafe { cin_skipcomment(line, 0) }, 7);
            // Several block comments in a row are all skipped.
            let line = b"/*a*/ /*b*/ y";
            assert_eq!(unsafe { cin_skipcomment(line, 0) }, 12);
        });
    }

    #[test]
    fn cin_skipcomment_consumes_an_unterminated_block_comment() {
        with_hash_comment(0, || {
            let line = b"/* never closed";
            assert_eq!(unsafe { cin_skipcomment(line, 0) }, line.len());
        });
    }

    #[test]
    fn cin_skipcomment_stops_at_a_lone_slash() {
        with_hash_comment(0, || {
            // A single `/` is division, not a comment; the original
            // has already stepped past it when it finds out.
            assert_eq!(unsafe { cin_skipcomment(b"/ x", 0) }, 1);
        });
    }

    #[test]
    fn cin_skipcomment_honours_b_ind_hash_comment_and_its_space_rule() {
        // The `#` rule requires whitespace to have been skipped first
        // ("require a space before # to avoid recognizing $#array"),
        // so a `#` in column 0 is NOT treated as a comment.
        let line = b" # comment";
        with_hash_comment(1, || {
            assert_eq!(unsafe { cin_skipcomment(line, 0) }, line.len());
            // No preceding space -> not a comment, so the scan stops
            // at the `#` itself.
            assert_eq!(unsafe { cin_skipcomment(b"#x", 0) }, 0);
        });
        // With the option off the `#` is never a comment.
        with_hash_comment(0, || {
            assert_eq!(unsafe { cin_skipcomment(line, 0) }, 1);
        });
    }

    #[test]
    fn cin_nocode_is_true_for_blank_and_comment_only_text() {
        with_hash_comment(0, || {
            assert!(unsafe { cin_nocode(b"", 0) });
            assert!(unsafe { cin_nocode(b"    ", 0) });
            assert!(unsafe { cin_nocode(b"// just a comment", 0) });
            assert!(unsafe { cin_nocode(b"  /* block */  ", 0) });
        });
    }

    #[test]
    fn cin_nocode_is_false_when_real_code_follows() {
        with_hash_comment(0, || {
            assert!(!unsafe { cin_nocode(b"x = 1;", 0) });
            assert!(!unsafe { cin_nocode(b"  /* c */ x", 0) });
        });
    }

    #[test]
    fn skip_string_leaves_a_non_string_position_alone() {
        // Nothing string-like at `p`, so the index comes back unchanged.
        assert_eq!(skip_string(b"abc", 0), 0);
        assert_eq!(skip_string(b"abc", 1), 1);
    }

    #[test]
    fn skip_string_skips_a_double_quoted_string() {
        // `"abc"` - lands on the CLOSING quote, not past it, because
        // of the original's own "backup from NUL" step.
        assert_eq!(skip_string(b"\"abc\"", 0), 4);
    }

    #[test]
    fn skip_string_advances_through_an_unterminated_string() {
        // The header comment says an absent string returns the
        // argument unmodified, but that is only true when `p` does
        // not START a string. Once the opening quote is seen the
        // original has already advanced `p`, and an unterminated
        // string runs to the end of the line - then the "backup from
        // NUL" step leaves it on the last byte. Quirk preserved.
        assert_eq!(skip_string(b"\"abc", 0), 3);
    }

    #[test]
    fn skip_string_skips_concatenated_strings_as_one_run() {
        // "date""time" - the loop continues across the join.
        let line = b"\"date\"\"time\"";
        assert_eq!(skip_string(line, 0), line.len() - 1);
    }

    #[test]
    fn skip_string_skips_a_char_constant_and_its_escapes() {
        assert_eq!(skip_string(b"'a'", 0), 2);
        // '\n' - the backslash form.
        assert_eq!(skip_string(b"'\\n'", 0), 3);
        // A lone quote at end of line is not a char constant.
        assert_eq!(skip_string(b"'", 0), 0);
    }

    #[test]
    fn skip_string_does_not_recognize_an_octal_char_constant() {
        // The original's own digit loop overshoots: it advances `i`
        // while `p[i - 1]` is a digit, so it stops with `p[i - 1]`
        // ON the closing quote and `p[i]` one PAST it. The trailing
        // check then tests the wrong byte and fails, leaving `'\000'`
        // unrecognized despite the comment naming it. Faithfully
        // preserved rather than "fixed" - the non-octal `'\n'` form
        // above shows the same code path working as intended.
        assert_eq!(skip_string(b"'\\000'", 0), 0);
        assert_eq!(skip_string(b"'\\0'", 0), 0);
    }

    #[test]
    fn skip_string_handles_an_escaped_quote_inside_a_string() {
        // "a\"b" - the escaped quote must not end the string.
        let line = b"\"a\\\"b\"";
        assert_eq!(skip_string(line, 0), line.len() - 1);
    }

    #[test]
    fn skip_string_skips_a_cpp_raw_string() {
        // R"x(hi)x" - the delimiter is `x`, so only `)x"` ends it.
        let line = b"R\"x(hi)x\"";
        assert_eq!(skip_string(line, 0), line.len() - 1);
        // With no `(` there is no delimiter, so it is not a raw string.
        assert_eq!(skip_string(b"R\"xy", 0), 0);
    }

    #[test]
    fn is_pos_in_string_detects_a_position_inside_a_string() {
        // `"abc"` - index 2 is the `b`, inside the string.
        assert!(is_pos_in_string(b"\"abc\"", 2));
        // `x"y"` - index 2 is the `y`, inside the string.
        assert!(is_pos_in_string(b"x\"y\"", 2));
    }

    #[test]
    fn is_pos_in_string_is_false_outside_a_string() {
        // Plain text is never inside a string.
        assert!(!is_pos_in_string(b"abc", 1));
        // The OPENING quote itself is not "inside" - the loop's own
        // `(p - line) < col` bound exits before scanning it.
        assert!(!is_pos_in_string(b"\"abc\"", 0));
        // Text before a string starts.
        assert!(!is_pos_in_string(b"x\"y\"", 0));
    }

    #[test]
    fn cin_is_cinword_matches_a_keyword_followed_by_a_non_word_char() {
        assert!(cinword_with(DEFAULT_CINW, b"if (x)"));
        assert!(cinword_with(DEFAULT_CINW, b"else"));
        assert!(cinword_with(DEFAULT_CINW, b"do {"));
        assert!(cinword_with(DEFAULT_CINW, b"switch (c)"));
    }

    #[test]
    fn cin_is_cinword_skips_leading_whitespace() {
        assert!(cinword_with(DEFAULT_CINW, b"   while (1)"));
        assert!(cinword_with(DEFAULT_CINW, b"\t\tfor (;;)"));
    }

    #[test]
    fn cin_is_cinword_rejects_a_longer_identifier() {
        // "ifx"/"iffy" start with "if" but continue with word chars,
        // so they are NOT cinwords.
        for line in [&b"ifx"[..], b"iffy", b"switching", b"doing", b"forx"] {
            assert!(
                !cinword_with(DEFAULT_CINW, line),
                "{:?} should not match",
                std::string::String::from_utf8_lossy(line)
            );
        }
    }

    #[test]
    fn cin_is_cinword_rejects_an_unrelated_line() {
        assert!(!cinword_with(DEFAULT_CINW, b"return 0;"));
        assert!(!cinword_with(DEFAULT_CINW, b""));
        // An empty 'cinwords' can never match.
        assert!(!cinword_with(b"", b"if (x)"));
    }

    #[test]
    fn cin_is_cinword_or_condition_lets_a_non_word_final_char_match_anything() {
        // The trailing test is `!vim_iswordc(line[len]) ||
        // !vim_iswordc(line[len - 1])` - an OR, not the `&&` a reader
        // might expect. So an entry ending in a NON-word character
        // matches even when the text continues with a word char.
        assert!(cinword_with(b"if.", b"if.x"));
        // The same entry without its trailing '.' does not.
        assert!(!cinword_with(b"if", b"ifx"));
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
