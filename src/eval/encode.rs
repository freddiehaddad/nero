//! Translated from `src/nvim/eval/encode.c` (tractable core only).
//!
//! `encode.c` implements FOUR separate typval-to-something encoders
//! (`encode_vim_to_string`/`encode_vim_to_echo`/`encode_vim_to_json`/
//! `encode_vim_to_msgpack`), each a distinct instantiation of ONE
//! shared macro template (`eval/typval_encode.c.h`) with different
//! quoting/escaping rules - `json`/`msgpack` additionally support a
//! "special dictionary" INPUT format for round-tripping types
//! Vimscript can't natively represent (extra-precision integers,
//! binary strings, etc), controlled by the template's own
//! `TYPVAL_ENCODE_ALLOW_SPECIALS` flag.
//!
//! The "string" (`encode_vim_to_string`/`encode_tv2string`, backing
//! the `string()` builtin) and "echo" (`encode_vim_to_echo`/
//! `encode_tv2echo`, backing `:echo`'s own display and dozens of other
//! internal "stringify for display" call sites) backends are both
//! translated here - both have `TYPVAL_ENCODE_ALLOW_SPECIALS` set to
//! `false` in the original, so the entire "special dictionary" input
//! path (~150 lines of the shared template) never applies to either
//! and isn't modeled at all. `json`/`msgpack` remain separate,
//! not-yet-translated undertakings (`ALLOW_SPECIALS: true` for both).
//!
//! "string" and "echo" share the exact same scalar/container
//! formatting rules for EVERYTHING except self-reference text (see
//! below) - confirmed directly against a real `nvim` binary
//! (`v0.13.0-dev`), not assumed from reading the shared macro template
//! alone: `:echo [1, 'a']` and `string([1, 'a'])` both print
//! `[1, 'a']`. The ONE real difference lives at the THIN WRAPPER level
//! (`encode_tv2echo`, not the container-walking `encode_vim_to_echo`
//! itself): a top-level `String`/`Func` value bypasses the whole
//! encoder and uses its raw text directly, unquoted - `:echo 'hello'`
//! prints `hello`, not `'hello'`, but that SAME string nested inside a
//! container (`:echo [1, 'a']` above) still gets quoted, because it
//! isn't the top-level value that time.
//!
//! Self-referencing containers (`let l = [] | call add(l, l)`) are
//! handled via `copyID`-marking exactly like `eval.rs`'s own
//! `set_ref_in_ht`/`set_ref_in_list_items` GC mark-phase: an explicit
//! stack (`EncodeFrame`) walks List/Dict values iteratively instead
//! of recursing, for the same "avoid stack overflow on deeply-nested
//! structures" reason - verified directly via a dedicated test
//! walking 20,000 levels of list-in-list nesting, matching
//! `set_ref_in_list_items`'s own established verification precedent.
//! Self-reference itself is the ONE real formatting difference between
//! "string" and "echo" (`EncodeMode` selects between them): `string()`
//! renders it as `{E724@N}` uniformly for BOTH List and Dict, plus a
//! real one-time `emsg("E724: ...")` (omitted here - message display,
//! not tractable; the identical `{E724@N}` text is still produced);
//! `:echo` renders it as `[...@N]`/`{...@N}` (List-vs-Dict specific)
//! with NO `emsg` at all - a genuine behavioral difference in the
//! original itself, not an omission, confirmed directly against the
//! same real `nvim` binary for both: `string(l)`/`:echo l` for a list
//! containing itself print `[1, [{E724@0}]]`/`[1, [...@0]]`
//! respectively. Easy to misread as the same format on a first pass
//! through the macro definitions (the FIRST `TYPVAL_ENCODE_CONV_RECURSE`
//! override in the source backs "string"; a SECOND, differently-shaped
//! one a few lines later backs "echo") - this was double-checked
//! against the real binary specifically because of that risk.
//!
//! `Func`/`Partial` values `unimplemented!()` inside the shared
//! container walker - `string()`/`:echo` of a Funcref/Partial is a
//! narrow case, and `Partial`'s own "bound args"/"self dict"
//! sub-iteration would add a third, rarely-needed `EncodeFrame`
//! variant for comparatively little value; add if a real caller ever
//! needs it. A bare `VAR_FUNC` (no partial wrapper) IS implemented in
//! full for both backends, since it needed no extra stack-frame
//! machinery beyond what `encode_string_quoted` already provides -
//! its own exact output (`function('name')`) was also confirmed
//! against the same real `nvim` binary.

use crate::eval::typval_defs::{BlobT, DictT, ListT, ListitemT, TypvalT, TypvalValue};

/// One entry of the explicit stack used to walk a List/Dict without
/// recursion (`MPConvStackVal`, `eval/typval_encode.c.h`) - only the
/// two variants [`encode_vim_to_string`] needs (`kMPConvList`/
/// `kMPConvDict`); `kMPConvPartial` is not modeled, see this module's
/// own doc comment.
enum EncodeFrame {
    List {
        /// The list itself (`data.l.list`) - needed to recognize a
        /// deeper self-reference to this SAME list (see
        /// [`encode_backref`]).
        list: *mut ListT,
        li: *mut ListitemT,
        saved_copy_id: i32,
    },
    Dict {
        /// The dict itself (`data.d.dict`) - same reason as `List`'s
        /// own `list` field above.
        dict: *mut DictT,
        /// Index into `dv_hashtab.ht_array` of the next bucket to
        /// examine (`hi`, a raw pointer in the original - an index is
        /// used here instead since Rust can't easily keep a live
        /// reference into `dv_hashtab`'s own array across the
        /// `dv_index` lookups this loop also needs).
        idx: usize,
        /// Number of entries not yet emitted (`todo`).
        todo: usize,
        saved_copy_id: i32,
    },
}

/// Quote and escape a string the way `string()` does: wrapped in `'`,
/// with every embedded `'` doubled (`TYPVAL_ENCODE_CONV_STRING`,
/// `eval/encode.c`'s own "string" backend macro) - `None` matches a
/// null `char *`, rendered as `''`, exactly like the original's own
/// `buf_ == NULL` check.
fn encode_string_quoted(out: &mut Vec<u8>, s: Option<&[u8]>) {
    let Some(s) = s else {
        out.extend_from_slice(b"''");
        return;
    };
    out.push(b'\'');
    for &b in s {
        if b == b'\'' {
            out.push(b'\'');
        }
        out.push(b);
    }
    out.push(b'\'');
}

/// Render a Blob the way `string()` does: `0z` followed by two hex
/// digits per byte, with a `.` separator every 4 bytes
/// (`TYPVAL_ENCODE_CONV_BLOB`).
///
/// # Safety
/// `blob`, if non-null, must be a valid pointer to a live [`BlobT`].
unsafe fn encode_blob(out: &mut Vec<u8>, blob: *const BlobT) {
    // SAFETY: forwarded from this function's own safety doc.
    let len = unsafe { crate::eval::typval::tv_blob_len(blob) };
    out.extend_from_slice(b"0z");
    for i in 0..len {
        if i > 0 && i % 4 == 0 {
            out.push(b'.');
        }
        // SAFETY: forwarded from this function's own safety doc.
        let byte = unsafe { crate::eval::typval::tv_blob_get(blob, i) };
        out.extend_from_slice(format!("{byte:02X}").as_bytes());
    }
}

/// Render a Float the way `string()` does - unlike plain `%g`
/// (`tv_get_string`'s own Float handling), NaN/infinity render as
/// Vimscript expressions that reconstruct the same value
/// (`TYPVAL_ENCODE_CONV_FLOAT`).
fn encode_float(out: &mut Vec<u8>, f: f64) {
    if f.is_nan() {
        out.extend_from_slice(b"str2float('nan')");
    } else if f.is_infinite() {
        if f < 0.0 {
            out.push(b'-');
        }
        out.extend_from_slice(b"str2float('inf')");
    } else {
        out.extend_from_slice(&crate::eval::typval::fmt_g(f));
    }
}

/// Render a bare Funcref the way `string()` does: `function('name')`
/// (`TYPVAL_ENCODE_CONV_FUNC_START`/`_BEFORE_ARGS`/`_BEFORE_SELF`/
/// `_END`, called back-to-back with `len: 0`/`len: -1` for a plain,
/// non-partial `VAR_FUNC` - both conditions those macros gate on are
/// then false, so no extra `", "` separator is ever emitted here).
fn encode_func(out: &mut Vec<u8>, name: Option<&[u8]>) {
    out.extend_from_slice(b"function(");
    match name {
        // internal_error("string(): NULL function name") in the
        // original - an internal-invariant-violation path that should
        // never be reached by any real value; the original still
        // emits this exact malformed-looking (unterminated-quote)
        // literal in that case, faithfully reproduced here rather
        // than narrowed away.
        None => out.extend_from_slice(b"NULL"),
        Some(name) => encode_string_quoted(out, Some(name)),
    }
    out.push(b')');
}

/// Convert a single, already-known-scalar (or List/Dict-start) value,
/// pushing a new [`EncodeFrame`] onto `stack` for List/Dict instead of
/// recursing (`TYPVAL_ENCODE_CONVERT_ONE_VALUE`, shared by both the
/// "string" and "echo" backends - see this module's own doc comment -
/// `mode` selects only the one place they actually differ: the
/// self-reference text).
///
/// The original's own early-exit (`goto
/// typval_encode_stop_converting_one_item`, used by
/// `TYPVAL_ENCODE_CONV_DICT_START`/`_LIST_START` to let a backend skip
/// the rest of `TYPVAL_ENCODE_CONVERT_ONE_VALUE` after already fully
/// handling a value) is not modeled - both backends' own
/// `_DICT_START`/`_LIST_START` macros are plain, unconditional
/// `ga_append`s with no such early-exit, so this situation never
/// actually arises here.
///
/// # Safety
/// If `tv`'s value is `List`/`Dict`/`Blob`-typed with a non-null
/// pointer, that pointer must be valid, with every item/entry
/// reachable through it also holding valid values recursively.
unsafe fn convert_one_value(out: &mut Vec<u8>, stack: &mut Vec<EncodeFrame>, tv: &TypvalT, copy_id: i32, mode: EncodeMode) {
    match &tv.value {
        TypvalValue::String(s) => encode_string_quoted(out, s.as_deref()),
        TypvalValue::Number(n) => out.extend_from_slice(n.to_string().as_bytes()),
        TypvalValue::Float(f) => encode_float(out, *f),
        TypvalValue::Blob(b) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { encode_blob(out, *b) };
        }
        TypvalValue::Func(name) => encode_func(out, name.as_deref()),
        TypvalValue::Partial(_) => {
            unimplemented!(
                "encode_vim_to_string/encode_vim_to_echo: Partial values not yet \
                 translated - see this module's own doc comment"
            );
        }
        TypvalValue::List(l) => {
            // SAFETY: forwarded from this function's own safety doc.
            if l.is_null() || unsafe { crate::eval::typval::tv_list_len(*l) } == 0 {
                out.extend_from_slice(b"[]");
                return;
            }
            // SAFETY: forwarded from this function's own safety doc.
            let list = unsafe { &mut **l };
            let saved_copy_id = list.lv_copy_id;
            if saved_copy_id == copy_id {
                let backref = encode_backref(stack, EncodeRef::List(*l));
                mode.write_self_reference(out, backref, true);
                return;
            }
            list.lv_copy_id = copy_id;
            out.push(b'[');
            stack.push(EncodeFrame::List { list: *l, li: list.lv_first, saved_copy_id });
        }
        TypvalValue::Dict(d) => {
            // SAFETY: forwarded from this function's own safety doc.
            if d.is_null() || unsafe { (**d).dv_hashtab.ht_used } == 0 {
                out.extend_from_slice(b"{}");
                return;
            }
            // SAFETY: forwarded from this function's own safety doc.
            let dict = unsafe { &mut **d };
            let saved_copy_id = dict.dv_copy_id;
            if saved_copy_id == copy_id {
                let backref = encode_backref(stack, EncodeRef::Dict(*d));
                mode.write_self_reference(out, backref, false);
                return;
            }
            dict.dv_copy_id = copy_id;
            let todo = dict.dv_hashtab.ht_used;
            out.push(b'{');
            stack.push(EncodeFrame::Dict { dict: *d, idx: 0, todo, saved_copy_id });
        }
        TypvalValue::Bool(b) => {
            out.extend_from_slice(if *b == crate::eval::typval_defs::BoolVarValue::True { b"v:true" } else { b"v:false" });
        }
        TypvalValue::Special(_) => out.extend_from_slice(b"v:null"),
        TypvalValue::Unknown => {
            unreachable!("encode_vim_to_string/encode_vim_to_echo: VAR_UNKNOWN should never reach a real caller")
        }
    }
}

/// Which backend [`convert_one_value`]/the main driving loop is
/// producing output for - the ONLY place the "string" and "echo"
/// backends actually differ (every scalar/container formatting rule
/// is otherwise identical, confirmed directly against a real `nvim`
/// binary - see this module's own doc comment).
#[derive(Clone, Copy)]
enum EncodeMode {
    /// `encode_vim_to_string` - self-reference renders as `{E724@N}`
    /// uniformly for List and Dict, plus a real one-time `emsg`
    /// (omitted here - message display, not tractable).
    Str,
    /// `encode_vim_to_echo` - self-reference renders as `[...@N]`/
    /// `{...@N}` (List-vs-Dict specific), no `emsg` at all.
    Echo,
}

impl EncodeMode {
    /// Write this mode's own self-reference text for a List (`is_list:
    /// true`) or Dict (`is_list: false`) - see [`EncodeMode`]'s own doc
    /// comment.
    fn write_self_reference(self, out: &mut Vec<u8>, backref: usize, is_list: bool) {
        match self {
            EncodeMode::Str => out.extend_from_slice(format!("{{E724@{backref}}}").as_bytes()),
            EncodeMode::Echo if is_list => out.extend_from_slice(format!("[...@{backref}]").as_bytes()),
            EncodeMode::Echo => out.extend_from_slice(format!("{{...@{backref}}}").as_bytes()),
        }
    }
}


/// Either a List or a Dict pointer, compared for identity by
/// [`encode_backref`] (a small helper enum - the original relies on a
/// runtime `conv_type` tag plus a raw `void *`/union field instead;
/// modeled as a real Rust enum here so the comparison itself stays
/// exhaustive and type-safe).
enum EncodeRef {
    List(*mut ListT),
    Dict(*mut DictT),
}

/// Compute the `@N` backref index for a self-referencing List/Dict:
/// the index (from the BOTTOM of the stack, i.e. its oldest entry) of
/// the FIRST frame whose own list/dict pointer equals `val`
/// (`TYPVAL_ENCODE_CONV_RECURSE`'s own `backref` scan). Falls back to
/// `stack.len()` if somehow not found (matching the original's own
/// for-loop, which likewise ends at `kv_size(*mpstack)` when the loop
/// runs off the end without `break`ing).
fn encode_backref(stack: &[EncodeFrame], val: EncodeRef) -> usize {
    for (i, frame) in stack.iter().enumerate() {
        let matches = match (frame, &val) {
            (EncodeFrame::List { list, .. }, EncodeRef::List(v)) => std::ptr::eq(*list, *v),
            (EncodeFrame::Dict { dict, .. }, EncodeRef::Dict(v)) => std::ptr::eq(*dict, *v),
            _ => false,
        };
        if matches {
            return i;
        }
    }
    stack.len()
}

/// Convert `top_tv` into the same textual representation `string(tv)`/
/// `:echo tv` would produce, depending on `mode`
/// (`TYPVAL_ENCODE_ENCODE`, shared driving loop for both the "string"
/// and "echo" backends).
///
/// Drives `convert_one_value` in a loop exactly like the original's
/// own `while (kv_size(mpstack))` - each iteration either finishes a
/// List/Dict frame (popping it and emitting its closing bracket/
/// brace) or advances one and converts its next item/entry.
///
/// # Safety
/// If `top_tv`'s value is `List`/`Dict`/`Blob`-typed with a non-null
/// pointer, that pointer must be valid, with every item/entry
/// reachable through it also holding valid values recursively (same
/// contract as `tv_clear_simple`/every other typval-tree-walking
/// function in this crate).
unsafe fn encode_generic(top_tv: &TypvalT, mode: EncodeMode) -> Vec<u8> {
    let mut out = Vec::new();
    let copy_id = crate::eval::eval::get_copy_id();
    let mut stack: Vec<EncodeFrame> = Vec::new();

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { convert_one_value(&mut out, &mut stack, top_tv, copy_id, mode) };

    while let Some(frame) = stack.last_mut() {
        let next_tv: *const TypvalT = match frame {
            EncodeFrame::Dict { dict, idx, todo, saved_copy_id } => {
                if *todo == 0 {
                    let dict = *dict;
                    let saved_copy_id = *saved_copy_id;
                    stack.pop();
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { (*dict).dv_copy_id = saved_copy_id };
                    out.push(b'}');
                    continue;
                }
                let dict = *dict;
                // SAFETY: forwarded from this function's own safety doc.
                let full_todo = unsafe { (*dict).dv_hashtab.ht_used };
                if *todo != full_todo {
                    out.extend_from_slice(b", ");
                }
                // SAFETY: forwarded from this function's own safety doc.
                let array = unsafe { (*dict).dv_hashtab.ht_array.as_slice() };
                while crate::hashtab::hashitem_empty(&array[*idx]) {
                    *idx += 1;
                }
                let hi_key = array[*idx].hi_key as usize;
                *todo -= 1;
                *idx += 1;
                // SAFETY: forwarded from this function's own safety doc.
                let di = *unsafe { &(*dict).dv_index }
                    .get(&hi_key)
                    .expect("dv_index must track every live hi_key - see DictT's own doc comment");
                // SAFETY: forwarded from this function's own safety doc.
                let di_key = unsafe { &(*di).di_key };
                // di_key always carries a trailing NUL terminator
                // (matching hi_key's C-string contract - see
                // tv_dict_item_alloc's own doc comment), which the
                // real key text does NOT include - strip it here,
                // matching this crate's own established idiom (e.g.
                // tv_dict_equal's own di_key handling).
                encode_string_quoted(&mut out, Some(&di_key[..di_key.len() - 1]));
                out.extend_from_slice(b": ");
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { std::ptr::addr_of!((*di).di_tv) }
            }
            EncodeFrame::List { list, li, saved_copy_id } => {
                if li.is_null() {
                    let list = *list;
                    let saved_copy_id = *saved_copy_id;
                    stack.pop();
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { (*list).lv_copy_id = saved_copy_id };
                    out.push(b']');
                    continue;
                }
                let list = *list;
                let cur_li = *li;
                // SAFETY: forwarded from this function's own safety doc.
                if !std::ptr::eq(cur_li, unsafe { (*list).lv_first }) {
                    out.extend_from_slice(b", ");
                }
                // SAFETY: forwarded from this function's own safety doc.
                *li = unsafe { (*cur_li).li_next };
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { std::ptr::addr_of!((*cur_li).li_tv) }
            }
        };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { convert_one_value(&mut out, &mut stack, &*next_tv, copy_id, mode) };
    }

    out
}

/// Convert `top_tv` into the same textual representation `string(tv)`
/// would produce (`encode_vim_to_string`, specialized directly for
/// the "string" backend - see this module's own doc comment for why
/// the other 2 backends (`json`/`msgpack`) aren't modeled).
///
/// # Safety
/// Same as `encode_generic`.
pub unsafe fn encode_vim_to_string(top_tv: &TypvalT) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { encode_generic(top_tv, EncodeMode::Str) }
}

/// Public entry point matching the original's own thin wrapper
/// (`encode_tv2string`) - the original also returns the result's own
/// byte length via an `out`-parameter, redundant with `Vec<u8>::len`
/// here, so it's dropped.
///
/// # Safety
/// Same as [`encode_vim_to_string`].
#[must_use]
pub unsafe fn encode_tv2string(tv: &TypvalT) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { encode_vim_to_string(tv) }
}

/// Convert `top_tv` into the same textual representation `:echo tv`
/// would produce (`encode_vim_to_echo`, the "echo" backend) - every
/// scalar/container formatting rule is identical to
/// [`encode_vim_to_string`] EXCEPT self-reference (`[...@N]`/`{...@N}`
/// here, list-vs-dict specific, vs `encode_vim_to_string`'s uniform
/// `{E724@N}` - see this module's own doc comment for how this was
/// confirmed against a real `nvim` binary) - no `emsg` is produced for
/// self-reference here either way (`encode_vim_to_string`'s own,
/// omitted per this crate's established message-display policy;
/// `encode_vim_to_echo`'s own `TYPVAL_ENCODE_CONV_RECURSE` override
/// genuinely never calls `emsg` at all in the original, confirmed the
/// same way - a real behavioral difference from `string()`, not an
/// omission).
///
/// # Safety
/// Same as `encode_generic`.
pub unsafe fn encode_vim_to_echo(top_tv: &TypvalT) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { encode_generic(top_tv, EncodeMode::Echo) }
}

/// Public entry point matching the original's own thin wrapper
/// (`encode_tv2echo`) - unlike every other entry point in this module,
/// this one has its OWN special case: a top-level `String`/`Func`
/// value bypasses the whole encoder and uses its raw text directly
/// (no quotes) - confirmed directly against a real `nvim` binary:
/// `:echo 'hello'` prints `hello` (unquoted), but `:echo [1, 'a']`
/// prints `[1, 'a']` (quoted) - the SAME nested string, quoted only
/// because it isn't the TOP-level value. `Func`'s own bare name (no
/// `function(...)` wrapper) is used the same way, matching the
/// original's own `tv->vval.v_string` reuse for both `VAR_STRING`/
/// `VAR_FUNC` at this wrapper layer specifically (`encode.h`'s own
/// documented layout coincidence, not a mistake).
///
/// # Safety
/// Same as [`encode_vim_to_echo`].
#[must_use]
pub unsafe fn encode_tv2echo(tv: &TypvalT) -> Vec<u8> {
    match &tv.value {
        TypvalValue::String(s) | TypvalValue::Func(s) => s.clone().unwrap_or_default(),
        // SAFETY: forwarded from this function's own safety doc.
        _ => unsafe { encode_vim_to_echo(tv) },
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn s(tv: &TypvalT) -> String {
        // SAFETY: test-only values, always genuinely valid.
        String::from_utf8(unsafe { encode_vim_to_string(tv) }).unwrap()
    }

    fn num(n: i64) -> TypvalT {
        TypvalT { value: TypvalValue::Number(n), ..Default::default() }
    }

    fn float(f: f64) -> TypvalT {
        TypvalT { value: TypvalValue::Float(f), ..Default::default() }
    }

    fn string(v: &[u8]) -> TypvalT {
        TypvalT { value: TypvalValue::String(Some(v.to_vec())), ..Default::default() }
    }

    // All expected outputs in this module were cross-checked directly
    // against a real `nvim` binary (v0.13.0-dev) via `echo string(...)`,
    // not assumed from reading the C source alone - see this module's
    // own doc comment.

    #[test]
    fn number_renders_plain_decimal() {
        assert_eq!(s(&num(42)), "42");
        assert_eq!(s(&num(-7)), "-7");
        assert_eq!(s(&num(0)), "0");
    }

    #[test]
    fn float_renders_with_g_formatting() {
        assert_eq!(s(&float(2.25)), "2.25");
    }

    #[test]
    fn float_nan_and_infinity_use_str2float_syntax() {
        assert_eq!(s(&float(f64::NAN)), "str2float('nan')");
        assert_eq!(s(&float(f64::INFINITY)), "str2float('inf')");
        assert_eq!(s(&float(f64::NEG_INFINITY)), "-str2float('inf')");
    }

    #[test]
    fn string_is_single_quoted_with_doubled_embedded_quotes() {
        assert_eq!(s(&string(b"hello")), "'hello'");
        assert_eq!(s(&string(b"it's")), "'it''s'");
        assert_eq!(s(&string(b"")), "''");
    }

    #[test]
    fn null_string_renders_as_empty_quotes() {
        let tv = TypvalT { value: TypvalValue::String(None), ..Default::default() };
        assert_eq!(s(&tv), "''");
    }

    #[test]
    fn bool_and_special_render_as_vim_literals() {
        let t = TypvalT { value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True), ..Default::default() };
        let f = TypvalT { value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::False), ..Default::default() };
        let n = TypvalT { value: TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null), ..Default::default() };
        assert_eq!(s(&t), "v:true");
        assert_eq!(s(&f), "v:false");
        assert_eq!(s(&n), "v:null");
    }

    #[test]
    fn func_renders_as_function_call_with_quoted_name() {
        let tv = TypvalT { value: TypvalValue::Func(Some(b"tr".to_vec())), ..Default::default() };
        assert_eq!(s(&tv), "function('tr')");
    }

    #[test]
    fn blob_renders_as_0z_hex_with_dot_every_4_bytes() {
        let _lock = crate::globals::global_state_test_lock();
        let b = crate::eval::typval::tv_blob_alloc();
        let empty_tv = TypvalT { value: TypvalValue::Blob(b), ..Default::default() };
        assert_eq!(s(&empty_tv), "0z");

        unsafe { (*b).bv_ga.ga_concat_len(&[0x01, 0x02]) };
        assert_eq!(s(&TypvalT { value: TypvalValue::Blob(b), ..Default::default() }), "0z0102");

        unsafe { (*b).bv_ga.ga_concat_len(&[0x03, 0x04, 0x05]) };
        assert_eq!(s(&TypvalT { value: TypvalValue::Blob(b), ..Default::default() }), "0z01020304.05");

        unsafe { crate::eval::typval::tv_blob_free(b) };
    }

    #[test]
    fn list_empty_and_with_items() {
        let _lock = crate::globals::global_state_test_lock();
        let l = crate::eval::typval::tv_list_alloc(0);
        assert_eq!(s(&TypvalT { value: TypvalValue::List(l), ..Default::default() }), "[]");
        unsafe { crate::eval::typval::tv_list_unref(l) };

        let l = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(l, 1);
            crate::eval::typval::tv_list_append_number(l, 2);
            crate::eval::typval::tv_list_append_number(l, 3);
        }
        assert_eq!(s(&TypvalT { value: TypvalValue::List(l), ..Default::default() }), "[1, 2, 3]");
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn list_of_mixed_types() {
        let _lock = crate::globals::global_state_test_lock();
        let l = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(l, 1);
            crate::eval::typval::tv_list_append_string(l, Some(b"a"));
            crate::eval::typval::tv_list_append_tv(l, &float(2.5));
        }
        assert_eq!(s(&TypvalT { value: TypvalValue::List(l), ..Default::default() }), "[1, 'a', 2.5]");
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn dict_empty_and_with_entries() {
        let _lock = crate::globals::global_state_test_lock();
        let d = crate::eval::typval::tv_dict_alloc();
        assert_eq!(s(&TypvalT { value: TypvalValue::Dict(d), ..Default::default() }), "{}");
        unsafe { crate::eval::typval::tv_dict_unref(d) };

        let d = crate::eval::typval::tv_dict_alloc();
        unsafe {
            (*d).dv_refcount += 1;
            let a = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*a).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *d, a);
        }
        assert_eq!(s(&TypvalT { value: TypvalValue::Dict(d), ..Default::default() }), "{'a': 1}");

        unsafe {
            let b = crate::eval::typval::tv_dict_item_alloc(b"b");
            (*b).di_tv.value = TypvalValue::String(Some(b"x".to_vec()));
            crate::eval::typval::tv_dict_add(&mut *d, b);
        }
        assert_eq!(s(&TypvalT { value: TypvalValue::Dict(d), ..Default::default() }), "{'a': 1, 'b': 'x'}");
        unsafe { crate::eval::typval::tv_dict_unref(d) };
    }

    #[test]
    fn nested_dict_in_list_in_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let inner_dict = crate::eval::typval::tv_dict_alloc();
        unsafe {
            (*inner_dict).dv_refcount += 1;
            let b = crate::eval::typval::tv_dict_item_alloc(b"b");
            (*b).di_tv.value = TypvalValue::Number(2);
            crate::eval::typval::tv_dict_add(&mut *inner_dict, b);
        }
        let inner_list = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_ref(inner_list);
            crate::eval::typval::tv_list_append_number(inner_list, 1);
            // tv_copy (inside tv_list_append_tv) increments inner_dict's
            // own refcount, matching the +1 already taken above for
            // this local `inner_dict` variable itself - so inner_dict
            // ends up with refcount 2, exactly one per real owner
            // (this local binding, and the list item).
            crate::eval::typval::tv_list_append_tv(
                inner_list,
                &TypvalT { value: TypvalValue::Dict(inner_dict), ..Default::default() },
            );
        }
        let outer = crate::eval::typval::tv_dict_alloc();
        unsafe {
            (*outer).dv_refcount += 1;
            let a = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*a).di_tv.value = TypvalValue::List(inner_list);
            crate::eval::typval::tv_dict_add(&mut *outer, a);
        }
        assert_eq!(s(&TypvalT { value: TypvalValue::Dict(outer), ..Default::default() }), "{'a': [1, {'b': 2}]}");
        unsafe {
            crate::eval::typval::tv_dict_unref(outer);
            crate::eval::typval::tv_dict_unref(inner_dict); // release this local binding's own reference.
        }
    }

    #[test]
    fn self_referencing_list_renders_e724_backref() {
        let _lock = crate::globals::global_state_test_lock();
        let l = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_ref(l);
            crate::eval::typval::tv_list_append_number(l, 1);
            crate::eval::typval::tv_list_append_tv(l, &TypvalT { value: TypvalValue::List(l), ..Default::default() });
        }
        assert_eq!(s(&TypvalT { value: TypvalValue::List(l), ..Default::default() }), "[1, {E724@0}]");
        // A genuinely self-referential list can NEVER naturally reach
        // refcount 0 via tv_list_unref (its own second item holds a
        // reference to itself forever) - force-free it directly instead
        // (freeing each item's own struct without following its value
        // through tv_clear_simple, then the list itself via
        // tv_list_free_list, which ignores both refcount and contained
        // items). Leaving it un-freed would permanently register it in
        // the shared GC_FIRST_LIST linked list, breaking later tests
        // that assert "no list is live before this test" (the exact
        // regression this test's own earlier draft caused - confirmed
        // via 8/8 full-suite runs failing before this cleanup was
        // added).
        unsafe {
            let mut item = (*l).lv_first;
            while !item.is_null() {
                let next = (*item).li_next;
                drop(Box::from_raw(item));
                item = next;
            }
            crate::eval::typval::tv_list_free_list(l);
        }
    }

    #[test]
    fn self_referencing_dict_renders_e724_backref() {
        let _lock = crate::globals::global_state_test_lock();
        let d = crate::eval::typval::tv_dict_alloc();
        unsafe {
            (*d).dv_refcount += 1;
            let a = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*a).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *d, a);
            let b = crate::eval::typval::tv_dict_item_alloc(b"b");
            (*b).di_tv.value = TypvalValue::Dict(d);
            (*d).dv_refcount += 1;
            crate::eval::typval::tv_dict_add(&mut *d, b);
        }
        assert_eq!(s(&TypvalT { value: TypvalValue::Dict(d), ..Default::default() }), "{'a': 1, 'b': {E724@0}}");
        // Same reasoning as self_referencing_list_renders_e724_backref's
        // own cleanup: force-free directly (bypassing tv_dict_item_free's
        // own tv_clear_simple call, which would just decrement d's
        // refcount without ever reaching 0) rather than leaking a
        // permanent GC_FIRST_DICT entry.
        unsafe {
            for item in (*d).dv_index.values().copied() {
                drop(Box::from_raw(item));
            }
            crate::eval::typval::tv_dict_free_dict(d);
        }
    }

    #[test]
    fn deeply_nested_list_does_not_overflow_the_stack() {
        let _lock = crate::globals::global_state_test_lock();
        // Build list_19999 = [19999], list_19998 = [list_19999], ...,
        // list_0 = [list_1] - 20,000 levels of list-in-list nesting,
        // matching set_ref_in_list_items's own established
        // verification precedent for the same "avoid recursion" claim.
        //
        // Each freshly-`tv_list_alloc`'d list starts at refcount 0;
        // `tv_list_append_tv` embedding it into its own parent (via
        // `tv_copy`) takes the ONE reference it will ever have (the
        // parent's own list-item slot) - nothing else references it,
        // so no extra `tv_list_unref` belongs here (that would drop
        // it to 0 and free it immediately, leaving the parent with a
        // dangling pointer).
        let mut innermost = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_append_number(innermost, 19999) };
        for _ in 0..19999 {
            let l = crate::eval::typval::tv_list_alloc(1);
            unsafe {
                crate::eval::typval::tv_list_append_tv(l, &TypvalT { value: TypvalValue::List(innermost), ..Default::default() });
            }
            innermost = l;
        }
        let result = s(&TypvalT { value: TypvalValue::List(innermost), ..Default::default() });
        // 20,000 levels of nesting means 20,000 "[" then "19999" then
        // 20,000 "]" - not a handful of brackets near the value.
        assert_eq!(result.len(), 20_000 + "19999".len() + 20_000);
        assert_eq!(&result[..20_000], "[".repeat(20_000));
        assert_eq!(&result[20_000..20_005], "19999");
        assert_eq!(&result[20_005..], "]".repeat(20_000));

        // Free the whole 20,000-list chain WITHOUT tv_list_unref: that
        // was confirmed (via a real stack-overflow crash while writing
        // this test) to recurse 20,000 native call frames deep through
        // tv_list_free_contents -> tv_clear_simple -> tv_list_unref ->
        // tv_list_free -> ... - a genuine, pre-existing gap logged as
        // SQL todo repo-health-tv-list-free-recursion (unlike THIS
        // function's own iterative, explicit-stack EncodeFrame walk,
        // which this test already proved handles the exact same depth
        // without issue). Leaving the chain unfreed isn't an option
        // either: every list stays registered in the shared
        // GC_FIRST_LIST linked list for the rest of the test process's
        // lifetime, breaking later tests that assert "no list is live
        // before this test" (confirmed the hard way: this exact
        // scenario made 3 unrelated GC-linked-list tests fail 8/8 times
        // before this cleanup loop was added). So: walk the chain
        // iteratively (collecting every list pointer first, itself
        // safe since it only follows `lv_first`/`li_tv` reads, no
        // freeing yet), then free each list's own single item directly
        // (a plain `Box::from_raw`+`drop` - safe because a `TypvalT`
        // holding `TypvalValue::List(*mut ListT)` has no `Drop` impact
        // of its own on that raw pointer, so this does NOT recurse into
        // the nested list) before unlinking/freeing the list itself via
        // `tv_list_free_list` (which ignores refcount and contained
        // items entirely, exactly what's needed here).
        let mut chain = vec![innermost];
        loop {
            let cur = *chain.last().unwrap();
            // SAFETY: every list in `chain` was allocated above and is
            // still live (nothing has freed anything yet).
            let item = unsafe { (*cur).lv_first };
            if item.is_null() {
                break;
            }
            // SAFETY: forwarded from the same reasoning as above.
            let TypvalValue::List(next) = (unsafe { &(*item).li_tv }).value else { break };
            chain.push(next);
        }
        for l in chain {
            // SAFETY: forwarded from this test's own reasoning above.
            unsafe {
                let item = (*l).lv_first;
                if !item.is_null() {
                    drop(Box::from_raw(item));
                }
                crate::eval::typval::tv_list_free_list(l);
            }
        }
    }

    #[test]
    fn encode_tv2string_matches_encode_vim_to_string() {
        // SAFETY: test-only values, always genuinely valid.
        assert_eq!(unsafe { encode_tv2string(&num(42)) }, unsafe { encode_vim_to_string(&num(42)) });
    }

    // --- encode_vim_to_echo / encode_tv2echo ---
    //
    // All expected outputs cross-checked directly against a real
    // `nvim` binary (v0.13.0-dev) via `:echo`, the same way as the
    // "string" backend's own tests above.

    fn echo(tv: &TypvalT) -> String {
        // SAFETY: test-only values, always genuinely valid.
        String::from_utf8(unsafe { encode_tv2echo(tv) }).unwrap()
    }

    #[test]
    fn echo_top_level_string_is_unquoted() {
        assert_eq!(echo(&string(b"hello")), "hello");
        assert_eq!(echo(&string(b"")), "");
    }

    #[test]
    fn echo_top_level_func_is_bare_name() {
        let tv = TypvalT { value: TypvalValue::Func(Some(b"tr".to_vec())), ..Default::default() };
        assert_eq!(echo(&tv), "tr");
    }

    #[test]
    fn echo_nested_string_and_func_are_still_quoted_wrapped() {
        let _lock = crate::globals::global_state_test_lock();
        let l = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_append_tv(l, &TypvalT { value: TypvalValue::Func(Some(b"tr".to_vec())), ..Default::default() }) };
        assert_eq!(echo(&TypvalT { value: TypvalValue::List(l), ..Default::default() }), "[function('tr')]");
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn echo_scalars_match_string_backend() {
        // Every scalar/container formatting rule (besides top-level
        // String/Func and self-reference) is shared between the two
        // backends - see this module's own doc comment.
        assert_eq!(echo(&num(42)), "42");
        assert_eq!(echo(&float(2.25)), "2.25");
        assert_eq!(echo(&float(f64::NAN)), "str2float('nan')");
        let t = TypvalT { value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True), ..Default::default() };
        assert_eq!(echo(&t), "v:true");
        let n = TypvalT { value: TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null), ..Default::default() };
        assert_eq!(echo(&n), "v:null");
    }

    #[test]
    fn echo_list_and_dict_containers() {
        let _lock = crate::globals::global_state_test_lock();
        let l = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(l, 1);
            crate::eval::typval::tv_list_append_string(l, Some(b"a"));
        }
        assert_eq!(echo(&TypvalT { value: TypvalValue::List(l), ..Default::default() }), "[1, 'a']");
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn echo_self_referencing_list_uses_bracket_backref_not_e724() {
        let _lock = crate::globals::global_state_test_lock();
        let l = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_ref(l);
            crate::eval::typval::tv_list_append_number(l, 1);
            crate::eval::typval::tv_list_append_number(l, 2);
            crate::eval::typval::tv_list_append_tv(l, &TypvalT { value: TypvalValue::List(l), ..Default::default() });
        }
        assert_eq!(echo(&TypvalT { value: TypvalValue::List(l), ..Default::default() }), "[1, 2, [...@0]]");
        // Same manual force-free reasoning as self_referencing_list_renders_e724_backref.
        unsafe {
            let mut item = (*l).lv_first;
            while !item.is_null() {
                let next = (*item).li_next;
                drop(Box::from_raw(item));
                item = next;
            }
            crate::eval::typval::tv_list_free_list(l);
        }
    }

    #[test]
    fn echo_self_referencing_dict_uses_brace_backref_not_e724() {
        let _lock = crate::globals::global_state_test_lock();
        let d = crate::eval::typval::tv_dict_alloc();
        unsafe {
            (*d).dv_refcount += 1;
            let a = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*a).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *d, a);
            let b = crate::eval::typval::tv_dict_item_alloc(b"b");
            (*b).di_tv.value = TypvalValue::Dict(d);
            (*d).dv_refcount += 1;
            crate::eval::typval::tv_dict_add(&mut *d, b);
        }
        assert_eq!(echo(&TypvalT { value: TypvalValue::Dict(d), ..Default::default() }), "{'a': 1, 'b': {...@0}}");
        // Same manual force-free reasoning as self_referencing_dict_renders_e724_backref.
        unsafe {
            for item in (*d).dv_index.values().copied() {
                drop(Box::from_raw(item));
            }
            crate::eval::typval::tv_dict_free_dict(d);
        }
    }

    #[test]
    fn encode_tv2echo_matches_encode_vim_to_echo_for_a_list() {
        let _lock = crate::globals::global_state_test_lock();
        let l = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_append_number(l, 7) };
        let tv = TypvalT { value: TypvalValue::List(l), ..Default::default() };
        // SAFETY: test-only values, always genuinely valid.
        assert_eq!(unsafe { encode_tv2echo(&tv) }, unsafe { encode_vim_to_echo(&tv) });
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }
}

