//! Translated from `src/nvim/testing.c` (tractable core only).
//!
//! `testing.c` implements the `assert_*()` builtin family used by
//! Vimscript test suites. Translated here: `assert_equal()`/
//! `assert_notequal()` (via `assert_equal_common`), `assert_true()`/
//! `assert_false()` (via `assert_bool`), `assert_report()`
//! (trivial), and `assert_inrange()`. `assert_match()`/
//! `assert_notmatch()` need the real regex engine (`pattern_match`,
//! confirmed globally blocked, matching `search.c`'s own already-
//! documented status). `assert_fails()`/`assert_beeps()`/
//! `assert_nobeep()`/`assert_exception()` need real Ex-command
//! execution/terminal-bell tracking, neither translated.
//! `assert_equalfile()`/`test_garbagecollect_now()` need file I/O
//! comparison/a real GC sweep, neither translated. None of these
//! `unimplemented!()` - the whole `f_assert_*` entry point for each is
//! simply not registered in `FUNCTIONS` yet, matching this crate's
//! usual "just don't translate it yet" treatment for a whole
//! not-yet-tractable function (as opposed to a partially-tractable one
//! that gets a real signature with an `unimplemented!()` branch
//! inside).
//!
//! `prepare_assert_error`'s own "script:line: " prefix (via
//! `estack_sfile()`/`SOURCING_LNUM` in the original) is omitted
//! entirely: `estack_sfile()` itself, and the underlying "exestack"
//! push/pop machinery whose top-of-stack `SOURCING_LNUM` reads from,
//! are both not yet translated (see `runtime_defs.rs`'s own
//! `EstackT`/`EstackArgT` doc comments) - and nothing in this crate
//! currently pushes any real exestack frame (there's no Ex-command
//! execution engine driving script/function sourcing yet), so this
//! prefix is provably empty for every currently-reachable call - not
//! a narrowing, matching this crate's established "the condition
//! guarding this branch is always false today" precedent.

use crate::eval::typval_defs::{DictT, TypvalT, TypvalValue};

/// Which assert_* check is being performed (`assert_type_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertType {
    Equal,
    NotEqual,
    Match,
    NotMatch,
    Fails,
    Other,
}

/// Append `p` (a single already-known character, as raw bytes) to
/// `gap`, escaping it if it's an unprintable ASCII control character
/// (`ga_concat_esc`). Multi-byte characters (`p.len() > 1`) are never
/// escaped, matching the original's own `clen > 1` early return.
fn ga_concat_esc(gap: &mut Vec<u8>, p: &[u8]) {
    if p.len() > 1 {
        gap.extend_from_slice(p);
        return;
    }
    match p[0] {
        crate::ascii_defs::BS => gap.extend_from_slice(b"\\b"),
        crate::ascii_defs::ESC => gap.extend_from_slice(b"\\e"),
        crate::ascii_defs::FF => gap.extend_from_slice(b"\\f"),
        crate::ascii_defs::NL => gap.extend_from_slice(b"\\n"),
        crate::ascii_defs::TAB => gap.extend_from_slice(b"\\t"),
        crate::ascii_defs::CAR => gap.extend_from_slice(b"\\r"),
        b'\\' => gap.extend_from_slice(b"\\\\"),
        c if c < b' ' || c == 0x7f => gap.extend_from_slice(format!("\\x{c:02x}").as_bytes()),
        c => gap.push(c),
    }
}

/// Append `s` to `gap`, escaping unprintable characters and
/// shortening runs of more than 20 consecutive identical characters
/// to `\[<char> occurs N times]` (`ga_concat_shorten_esc`).
///
/// Scans at the byte level rather than the original's own
/// `mb_cptr2char_adv`/`utf_ptr2char` multi-byte-character-aware walk:
/// comparing raw byte sequences of a character's own byte length
/// (`utf_ptr2len`) for equality is equivalent to comparing decoded
/// codepoints for any well-formed UTF-8 input - the same reasoning
/// already established elsewhere in this crate (e.g.
/// `eval_lit_string`'s own doc comment).
fn ga_concat_shorten_esc(gap: &mut Vec<u8>, s: Option<&[u8]>) {
    let Some(s) = s else {
        gap.extend_from_slice(b"NULL");
        return;
    };
    let mut p = 0;
    while p < s.len() {
        let clen = (crate::mbyte::utf_ptr2len(&s[p..]).max(1) as usize).min(s.len() - p);
        let ch = &s[p..p + clen];
        let mut same_len = 1;
        let mut q = p + clen;
        while q + clen <= s.len() && &s[q..q + clen] == ch {
            same_len += 1;
            q += clen;
        }
        if same_len > 20 {
            gap.extend_from_slice(b"\\[");
            ga_concat_esc(gap, ch);
            gap.extend_from_slice(b" occurs ");
            gap.extend_from_slice(same_len.to_string().as_bytes());
            gap.extend_from_slice(b" times]");
            p = q;
        } else {
            ga_concat_esc(gap, ch);
            p += clen;
        }
    }
}

/// Prepare a fresh error-message buffer for an assert_* failure,
/// adding the sourcing position (`prepare_assert_error`) - see this
/// module's own doc comment for why the position prefix itself is
/// never actually added.
pub(crate) fn prepare_assert_error() -> Vec<u8> {
    Vec::new()
}

/// Encode `tv` as `string()` would, but with dict entries that are
/// equal between `tv` and `other` dropped first (used by
/// [`fill_assert_error`]'s own dict-diffing, matching the original's
/// own `exp_tv->vval.v_dict = tv_dict_alloc(); ...` temporary-dict
/// swap) - returns the encoded bytes plus how many entries were
/// omitted for being equal. Builds and frees its own temporary,
/// fully-owned `DictT` (never mutating `tv`/`other` themselves, unlike
/// the original's own approach of temporarily overwriting `exp_tv`'s
/// own `vval.v_dict` field in place - functionally equivalent for the
/// caller's only real need here, the final encoded text, without that
/// approach's own subtle "who owns/frees the swapped-out original
/// dict pointer" question).
///
/// # Safety
/// `dict`/`other_dict` must be valid, non-null pointers to live
/// `DictT`s, with every entry's own value satisfying
/// [`crate::eval::encode::encode_tv2string`]'s own safety contract.
unsafe fn encode_dict_diff(dict: *mut DictT, other_dict: *mut DictT) -> (Vec<u8>, usize) {
    let diff = crate::eval::typval::tv_dict_alloc();
    let mut omitted = 0;
    // SAFETY: forwarded from this function's own safety doc.
    let items: Vec<_> = unsafe { &(*dict).dv_index }.values().copied().collect();
    for item in items {
        // SAFETY: forwarded from this function's own safety doc.
        let di_key = unsafe { &(*item).di_key };
        let key = &di_key[..di_key.len() - 1];
        // SAFETY: forwarded from this function's own safety doc.
        let other_item = unsafe { crate::eval::typval::tv_dict_find(Some(&mut *other_dict), key) };
        // SAFETY: forwarded from this function's own safety doc.
        let equal = other_item
            .is_some_and(|i2| unsafe { crate::eval::typval::tv_equal(&(*item).di_tv, &(*i2).di_tv, false) });
        if equal {
            omitted += 1;
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_dict_add_tv(&mut *diff, key, &(*item).di_tv) };
        }
    }
    // SAFETY: forwarded from this function's own safety doc.
    let encoded = unsafe { crate::eval::encode::encode_tv2string(&TypvalT { value: TypvalValue::Dict(diff), ..Default::default() }) };
    // SAFETY: `diff` was just allocated above, a fresh, fully-owned
    // allocation never shared with anything else.
    unsafe { crate::eval::typval::tv_dict_free(diff) };
    (encoded, omitted)
}

/// Same as [`encode_dict_diff`], but for the "got" side: includes
/// every entry not equal to `other_dict`'s own same-named entry
/// (matching [`encode_dict_diff`]'s own omission), PLUS every entry
/// present in `dict` but entirely absent from `other_dict` (the
/// original's own second loop, "Add items only present in got_d").
///
/// # Safety
/// Same as [`encode_dict_diff`].
unsafe fn encode_dict_diff_got(dict: *mut DictT, other_dict: *mut DictT) -> Vec<u8> {
    let diff = crate::eval::typval::tv_dict_alloc();
    // SAFETY: forwarded from this function's own safety doc.
    let items: Vec<_> = unsafe { &(*dict).dv_index }.values().copied().collect();
    for item in items {
        // SAFETY: forwarded from this function's own safety doc.
        let di_key = unsafe { &(*item).di_key };
        let key = &di_key[..di_key.len() - 1];
        // SAFETY: forwarded from this function's own safety doc.
        let other_item = unsafe { crate::eval::typval::tv_dict_find(Some(&mut *other_dict), key) };
        let equal = other_item
            // SAFETY: forwarded from this function's own safety doc.
            .is_some_and(|i2| unsafe { crate::eval::typval::tv_equal(&(*item).di_tv, &(*i2).di_tv, false) });
        if !equal {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_dict_add_tv(&mut *diff, key, &(*item).di_tv) };
        }
    }
    // SAFETY: forwarded from this function's own safety doc.
    let encoded = unsafe { crate::eval::encode::encode_tv2string(&TypvalT { value: TypvalValue::Dict(diff), ..Default::default() }) };
    // SAFETY: `diff` was just allocated above, a fresh, fully-owned
    // allocation never shared with anything else.
    unsafe { crate::eval::typval::tv_dict_free(diff) };
    encoded
}

/// Fill `gap` with information about an assert error
/// (`fill_assert_error`).
///
/// See this module's own doc comment for why `exp_tv`/`got_tv` are
/// never mutated here (unlike the original's own temporary in-place
/// dict-pointer swap) - the dict-diffing case builds its own, fully
/// separate, owned temporary dicts instead (see [`encode_dict_diff`]/
/// [`encode_dict_diff_got`]).
///
/// # Safety
/// If `opt_msg_tv`/`exp_tv`/`got_tv`'s value is `List`/`Dict`/`Blob`-
/// typed with a non-null pointer, that pointer must be valid, with
/// every item/entry reachable through it also holding valid values
/// recursively (same contract as `encode_tv2string`/`encode_tv2echo`).
unsafe fn fill_assert_error(
    gap: &mut Vec<u8>,
    opt_msg_tv: Option<&TypvalT>,
    exp_str: Option<&[u8]>,
    exp_tv: Option<&TypvalT>,
    got_tv: &TypvalT,
    atype: AssertType,
) {
    let opt_msg_is_empty = match opt_msg_tv.map(|tv| &tv.value) {
        None | Some(TypvalValue::Unknown) => true,
        Some(TypvalValue::String(s)) => s.as_deref().is_none_or(<[u8]>::is_empty),
        Some(_) => false,
    };
    if !opt_msg_is_empty {
        let opt_msg_tv = opt_msg_tv.expect("opt_msg_is_empty is true whenever opt_msg_tv is None");
        // SAFETY: forwarded from this function's own safety doc.
        gap.extend_from_slice(&unsafe { crate::eval::encode::encode_tv2echo(opt_msg_tv) });
        gap.extend_from_slice(b": ");
    }

    match atype {
        AssertType::Match | AssertType::NotMatch => gap.extend_from_slice(b"Pattern "),
        AssertType::NotEqual => gap.extend_from_slice(b"Expected not equal to "),
        _ => gap.extend_from_slice(b"Expected "),
    }

    // When comparing two dictionaries (and not just checking they
    // differ), drop the items that are equal first, so it's a lot
    // easier to see what differs - see encode_dict_diff/
    // encode_dict_diff_got's own doc comments for why this crate
    // builds its own separate, owned temporary dicts for this rather
    // than mutating exp_tv/got_tv in place like the original.
    let dict_diff_dicts = if exp_str.is_none() && atype != AssertType::NotEqual {
        match (exp_tv.map(|t| &t.value), &got_tv.value) {
            (Some(TypvalValue::Dict(exp_d)), TypvalValue::Dict(got_d)) if !exp_d.is_null() && !got_d.is_null() => {
                Some((*exp_d, *got_d))
            }
            _ => None,
        }
    } else {
        None
    };

    let mut omitted = 0;
    if let Some(exp_str) = exp_str {
        if atype == AssertType::Fails {
            gap.push(b'\'');
        }
        ga_concat_shorten_esc(gap, Some(exp_str));
        if atype == AssertType::Fails {
            gap.push(b'\'');
        }
    } else {
        let exp_tv = exp_tv.expect("exp_tv must be Some when exp_str is None, matching the original's own contract");
        let exp_encoded = if let Some((exp_d, got_d)) = dict_diff_dicts {
            // SAFETY: forwarded from this function's own safety doc.
            let (encoded, n) = unsafe { encode_dict_diff(exp_d, got_d) };
            omitted = n;
            encoded
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::encode::encode_tv2string(exp_tv) }
        };
        ga_concat_shorten_esc(gap, Some(&exp_encoded));
    }

    if atype != AssertType::NotEqual {
        match atype {
            AssertType::Match => gap.extend_from_slice(b" does not match "),
            AssertType::NotMatch => gap.extend_from_slice(b" does match "),
            _ => gap.extend_from_slice(b" but got "),
        }

        let got_encoded = if let Some((exp_d, got_d)) = dict_diff_dicts {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { encode_dict_diff_got(got_d, exp_d) }
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::encode::encode_tv2string(got_tv) }
        };
        ga_concat_shorten_esc(gap, Some(&got_encoded));

        if omitted != 0 {
            gap.extend_from_slice(format!(" - {omitted} equal item{} omitted", if omitted == 1 { "" } else { "s" }).as_bytes());
        }
    }
}

/// Append `gap`'s own contents to `v:errors`, creating it as an empty
/// List first if it isn't one already (`assert_error`, `eval/vars.c`
/// - kept here anyway since every real caller is in this same module).
///
/// # Safety
/// Forwarded from [`crate::eval::vars::set_vim_var_list`]/
/// [`crate::eval::typval::tv_list_append_string`]'s own safety docs.
pub(crate) unsafe fn assert_error(gap: &[u8]) {
    // SAFETY: forwarded from this function's own safety doc.
    let tv = unsafe { crate::eval::vars::get_vim_var_tv(crate::eval::vars::VimVarIndex::Errors) };
    // SAFETY: forwarded from this function's own safety doc.
    let is_list = matches!(unsafe { &(*tv).value }, TypvalValue::List(l) if !l.is_null());
    if !is_list {
        let l = crate::eval::typval::tv_list_alloc(1);
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::vars::set_vim_var_list(crate::eval::vars::VimVarIndex::Errors, l) };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        crate::eval::typval::tv_list_append_string(crate::eval::vars::get_vim_var_list(crate::eval::vars::VimVarIndex::Errors), Some(gap));
    }
}

/// Common logic for `assert_equal()`/`assert_notequal()`
/// (`assert_equal_common`). Returns `1` (an assertion was recorded as
/// FAILED, appended to `v:errors`) or `0` (the values compared as
/// expected), matching the original's own `int` return used directly
/// as `rettv->vval.v_number`.
///
/// # Safety
/// Forwarded from [`crate::eval::typval::tv_equal`]/
/// [`fill_assert_error`]'s own safety docs.
pub(crate) unsafe fn assert_equal_common(argvars: &[TypvalT], atype: AssertType) -> i64 {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::eval::typval::tv_equal(&argvars[0], &argvars[1], false) } != (atype == AssertType::Equal) {
        let mut ga = prepare_assert_error();
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { fill_assert_error(&mut ga, argvars.get(2), None, Some(&argvars[0]), &argvars[1], atype) };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { assert_error(&ga) };
        return 1;
    }
    0
}

/// Common logic for `assert_true()`/`assert_false()` (`assert_bool`).
///
/// # Safety
/// Same as [`assert_equal_common`].
pub(crate) unsafe fn assert_bool(argvars: &[TypvalT], is_true: bool) -> i64 {
    let mut error = false;
    let number_says_wrong = argvars[0].var_type() != crate::eval::typval_defs::VarType::Number
        || (crate::eval::typval::tv_get_number_chk(&argvars[0], Some(&mut error)) == 0) == is_true
        || error;
    let bool_says_wrong = argvars[0].var_type() != crate::eval::typval_defs::VarType::Bool
        || !matches!(&argvars[0].value, TypvalValue::Bool(b) if *b == if is_true { crate::eval::typval_defs::BoolVarValue::True } else { crate::eval::typval_defs::BoolVarValue::False });
    if number_says_wrong && bool_says_wrong {
        let mut ga = prepare_assert_error();
        let exp_str = if is_true { &b"True"[..] } else { &b"False"[..] };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { fill_assert_error(&mut ga, argvars.get(1), Some(exp_str), None, &argvars[0], AssertType::Other) };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { assert_error(&ga) };
        return 1;
    }
    0
}

/// `assert_inrange(lower, upper, actual[, msg])` - whether `actual` is
/// within `lower..=upper` (`assert_inrange`).
///
/// # Safety
/// Same as [`assert_equal_common`].
pub(crate) unsafe fn assert_inrange(argvars: &[TypvalT]) -> i64 {
    let is_float = argvars[0].var_type() == crate::eval::typval_defs::VarType::Float
        || argvars[1].var_type() == crate::eval::typval_defs::VarType::Float
        || argvars[2].var_type() == crate::eval::typval_defs::VarType::Float;
    if is_float {
        let lower = crate::eval::typval::tv_get_float(&argvars[0]);
        let upper = crate::eval::typval::tv_get_float(&argvars[1]);
        let actual = crate::eval::typval::tv_get_float(&argvars[2]);
        if actual < lower || actual > upper {
            let mut ga = prepare_assert_error();
            let mut exp_str = b"range ".to_vec();
            exp_str.extend_from_slice(&crate::eval::typval::fmt_g(lower));
            exp_str.extend_from_slice(b" - ");
            exp_str.extend_from_slice(&crate::eval::typval::fmt_g(upper));
            exp_str.push(b',');
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { fill_assert_error(&mut ga, argvars.get(3), Some(&exp_str), None, &argvars[2], AssertType::Other) };
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { assert_error(&ga) };
            return 1;
        }
    } else {
        let mut error = false;
        let lower = crate::eval::typval::tv_get_number_chk(&argvars[0], Some(&mut error));
        let upper = crate::eval::typval::tv_get_number_chk(&argvars[1], Some(&mut error));
        let actual = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error));
        if error {
            return 0;
        }
        if actual < lower || actual > upper {
            let mut ga = prepare_assert_error();
            let exp_str = format!("range {lower} - {upper},");
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { fill_assert_error(&mut ga, argvars.get(3), Some(exp_str.as_bytes()), None, &argvars[2], AssertType::Other) };
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { assert_error(&ga) };
            return 1;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(n: i64) -> TypvalT {
        TypvalT { value: TypvalValue::Number(n), ..Default::default() }
    }

    fn string(s: &[u8]) -> TypvalT {
        TypvalT { value: TypvalValue::String(Some(s.to_vec())), ..Default::default() }
    }

    /// Release whatever `v:errors` currently holds and reset it to
    /// `None` (`TypvalValue::List(null)`), matching this crate's own
    /// established "avoid leaking a List into the shared GC_FIRST_LIST
    /// linked list between tests" discipline (see `eval/encode.rs`'s
    /// own module doc comment for the regression this exact class of
    /// bug caused earlier this session).
    fn reset_v_errors() {
        unsafe {
            let tv = crate::eval::vars::get_vim_var_tv(crate::eval::vars::VimVarIndex::Errors);
            if let TypvalValue::List(l) = (*tv).value {
                if !l.is_null() {
                    crate::eval::typval::tv_list_unref(l);
                }
            }
            (*tv).value = TypvalValue::List(std::ptr::null_mut());
        }
    }

    /// Collect `v:errors`' own current contents as owned strings, for
    /// easy assertion.
    fn v_errors() -> Vec<String> {
        unsafe {
            let tv = crate::eval::vars::get_vim_var_tv(crate::eval::vars::VimVarIndex::Errors);
            let TypvalValue::List(l) = (*tv).value else { return Vec::new() };
            if l.is_null() {
                return Vec::new();
            }
            let mut out = Vec::new();
            let mut item = crate::eval::typval::tv_list_first(l);
            while !item.is_null() {
                out.push(String::from_utf8_lossy(&crate::eval::typval::tv_get_string(&(*item).li_tv)).into_owned());
                item = (*item).li_next;
            }
            out
        }
    }

    // All expected message texts below were cross-checked directly
    // against a real `nvim` binary (v0.13.0-dev) via a `-S script.vim`
    // batch of `call assert_equal(...)`/etc. followed by `for e in
    // v:errors | echo e | endfor` - minus the "script:line: " prefix
    // this crate never adds (see this module's own doc comment).

    #[test]
    fn ga_concat_esc_escapes_known_control_chars_and_passes_through_multibyte() {
        let mut gap = Vec::new();
        ga_concat_esc(&mut gap, b"\n");
        ga_concat_esc(&mut gap, b"\t");
        ga_concat_esc(&mut gap, b"\\");
        ga_concat_esc(&mut gap, b"a");
        ga_concat_esc(&mut gap, "é".as_bytes()); // 2-byte UTF-8.
        assert_eq!(gap, b"\\n\\t\\\\a\xc3\xa9");
    }

    #[test]
    fn ga_concat_shorten_esc_plain_text_is_unchanged() {
        let mut gap = Vec::new();
        ga_concat_shorten_esc(&mut gap, Some(b"hello world"));
        assert_eq!(gap, b"hello world");
    }

    #[test]
    fn ga_concat_shorten_esc_null_is_literal_null() {
        let mut gap = Vec::new();
        ga_concat_shorten_esc(&mut gap, None);
        assert_eq!(gap, b"NULL");
    }

    #[test]
    fn ga_concat_shorten_esc_shortens_runs_over_20() {
        let mut gap = Vec::new();
        let s = "a".repeat(25);
        ga_concat_shorten_esc(&mut gap, Some(s.as_bytes()));
        assert_eq!(gap, b"\\[a occurs 25 times]");
    }

    #[test]
    fn ga_concat_shorten_esc_run_of_20_or_fewer_is_not_shortened() {
        let mut gap = Vec::new();
        let s = "a".repeat(20);
        ga_concat_shorten_esc(&mut gap, Some(s.as_bytes()));
        assert_eq!(gap, s.as_bytes());
    }

    #[test]
    fn assert_equal_records_nothing_when_equal() {
        let _lock = crate::globals::global_state_test_lock();
        reset_v_errors();
        assert_eq!(unsafe { assert_equal_common(&[num(1), num(1)], AssertType::Equal) }, 0);
        assert_eq!(v_errors(), Vec::<String>::new());
        reset_v_errors();
    }

    #[test]
    fn assert_equal_records_a_message_when_not_equal() {
        let _lock = crate::globals::global_state_test_lock();
        reset_v_errors();
        assert_eq!(unsafe { assert_equal_common(&[num(1), num(2)], AssertType::Equal) }, 1);
        assert_eq!(v_errors(), vec!["Expected 1 but got 2".to_string()]);
        reset_v_errors();
    }

    #[test]
    fn assert_equal_with_custom_message_prefixes_it() {
        let _lock = crate::globals::global_state_test_lock();
        reset_v_errors();
        assert_eq!(unsafe { assert_equal_common(&[num(1), num(2), string(b"my custom msg")], AssertType::Equal) }, 1);
        assert_eq!(v_errors(), vec!["my custom msg: Expected 1 but got 2".to_string()]);
        reset_v_errors();
    }

    #[test]
    fn assert_notequal_records_a_message_when_equal() {
        let _lock = crate::globals::global_state_test_lock();
        reset_v_errors();
        assert_eq!(unsafe { assert_equal_common(&[num(1), num(1)], AssertType::NotEqual) }, 1);
        assert_eq!(v_errors(), vec!["Expected not equal to 1".to_string()]);
        reset_v_errors();
    }

    #[test]
    fn assert_equal_dict_diff_omits_equal_entries() {
        let _lock = crate::globals::global_state_test_lock();
        reset_v_errors();
        let exp_d = crate::eval::typval::tv_dict_alloc();
        let got_d = crate::eval::typval::tv_dict_alloc();
        unsafe {
            (*exp_d).dv_refcount += 1;
            (*got_d).dv_refcount += 1;
            let a1 = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*a1).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *exp_d, a1);
            let a2 = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*a2).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *got_d, a2);
            let b1 = crate::eval::typval::tv_dict_item_alloc(b"b");
            (*b1).di_tv.value = TypvalValue::Number(2);
            crate::eval::typval::tv_dict_add(&mut *exp_d, b1);
            let b2 = crate::eval::typval::tv_dict_item_alloc(b"b");
            (*b2).di_tv.value = TypvalValue::Number(3);
            crate::eval::typval::tv_dict_add(&mut *got_d, b2);
        }
        let exp_tv = TypvalT { value: TypvalValue::Dict(exp_d), ..Default::default() };
        let got_tv = TypvalT { value: TypvalValue::Dict(got_d), ..Default::default() };
        assert_eq!(unsafe { assert_equal_common(&[exp_tv, got_tv], AssertType::Equal) }, 1);
        assert_eq!(v_errors(), vec!["Expected {'b': 2} but got {'b': 3} - 1 equal item omitted".to_string()]);
        reset_v_errors();
        unsafe {
            crate::eval::typval::tv_dict_unref(exp_d);
            crate::eval::typval::tv_dict_unref(got_d);
        }
    }

    #[test]
    fn assert_true_and_false() {
        let _lock = crate::globals::global_state_test_lock();
        reset_v_errors();
        assert_eq!(unsafe { assert_bool(&[num(1)], true) }, 0);
        assert_eq!(unsafe { assert_bool(&[num(0)], true) }, 1);
        assert_eq!(v_errors(), vec!["Expected True but got 0".to_string()]);
        reset_v_errors();

        assert_eq!(unsafe { assert_bool(&[num(0)], false) }, 0);
        assert_eq!(unsafe { assert_bool(&[num(1)], false) }, 1);
        assert_eq!(v_errors(), vec!["Expected False but got 1".to_string()]);
        reset_v_errors();
    }

    #[test]
    fn assert_report_always_records_the_message() {
        let _lock = crate::globals::global_state_test_lock();
        reset_v_errors();
        let mut ga = prepare_assert_error();
        ga.extend_from_slice(b"custom message");
        unsafe { assert_error(&ga) };
        assert_eq!(v_errors(), vec!["custom message".to_string()]);
        reset_v_errors();
    }

    #[test]
    fn assert_inrange_number_in_and_out_of_range() {
        let _lock = crate::globals::global_state_test_lock();
        reset_v_errors();
        assert_eq!(unsafe { assert_inrange(&[num(1), num(10), num(5)]) }, 0);
        assert_eq!(unsafe { assert_inrange(&[num(1), num(10), num(20)]) }, 1);
        assert_eq!(v_errors(), vec!["Expected range 1 - 10, but got 20".to_string()]);
        reset_v_errors();
    }

    #[test]
    fn assert_inrange_float_out_of_range() {
        let _lock = crate::globals::global_state_test_lock();
        reset_v_errors();
        let float = |f: f64| TypvalT { value: TypvalValue::Float(f), ..Default::default() };
        assert_eq!(unsafe { assert_inrange(&[float(1.0), float(10.0), float(20.0)]) }, 1);
        assert_eq!(v_errors(), vec!["Expected range 1 - 10, but got 20".to_string()]);
        reset_v_errors();
    }

    #[test]
    fn assert_error_creates_v_errors_as_a_list_if_needed() {
        let _lock = crate::globals::global_state_test_lock();
        reset_v_errors();
        unsafe { assert_error(b"one") };
        unsafe { assert_error(b"two") };
        assert_eq!(v_errors(), vec!["one".to_string(), "two".to_string()]);
        reset_v_errors();
    }
}
