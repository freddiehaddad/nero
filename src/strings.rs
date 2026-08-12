//! Translated from `src/nvim/strings.c` (partial).
//!
//! `strings.c` is large (93KB) and mixes several unrelated concerns: a
//! handful of low-level byte-string utilities (translated here), a large
//! custom `vim_snprintf`/`vim_vsnprintf` implementation with positional
//! `$`-style format-argument support, and dozens of `f_*` Vimscript builtin
//! function implementations (`f_strlen`, `f_tr`, `f_trim`, etc.) that
//! belong with the eval engine (phase 5), not here. Only the low-level
//! utilities with no eval dependency are translated in this pass:
//! - `xstrnsave`, `vim_stricmp`, `vim_strnicmp`, `striequal`,
//!   `vim_strnicmp_asc`, `sort_strings`, `has_non_ascii`,
//!   `has_non_ascii_len`, `concat_str`; and now that `mbyte.c`'s
//!   `mb_toupper`/`mb_tolower`/`utf_ptr2char_info`/`utf_char2bytes` all
//!   exist: `vim_strup`, `vim_strsave_up`, `vim_strcpy_up`,
//!   `mb_strup_buf`, `strcase_save`; `vim_strchr` (re-examined:
//!   an earlier note claimed this needed `charset.c`'s real `g_chartab`/
//!   `option.c`, but re-reading the actual body shows it only needs
//!   `strchr`/`strstr`-equivalent byte/substring search plus the
//!   already-translated `utf_char2bytes` - no chartab dependency at all;
//!   used extremely widely - 380+ call sites - across the rest of the
//!   original source, so this was worth double-checking rather than
//!   trusting the stale note).
//!
//! `vim_strup`/`vim_strsave_up`/`mb_strup_buf`/`strcase_save` are all
//! self-bounding via NUL-scanning (matching the original's own
//! `strlen`-based sizing/`while (*p != NUL)` loops) rather than
//! operating on their input slice's full length verbatim - unlike
//! [`xstrnsave`] (a lower-level "copy exactly N bytes, embedded NULs
//! included" primitive taking an explicit length, which deliberately
//! does *not* stop at an embedded NUL, per its own doc comment). Each
//! returns/leaves its own trailing NUL byte, matching this crate's
//! established `Vec<u8>`-includes-its-own-NUL convention. `vim_strchr`
//! is the same way (stops at the first embedded NUL, matching real
//! `strchr`/`strstr` on a NUL-terminated C string), but returns a byte
//! offset (`Option<usize>`) rather than a NUL-terminated `Vec<u8>`,
//! matching this crate's established "index instead of a raw pointer
//! into the same buffer" convention (e.g. `path.rs`'s `path_tail`/
//! `get_past_head`).
//!
//! Also translated: `vim_strnsave_unquoted`/`del_trailing_spaces`
//! (as [`del_trailing_spaces_len`]) - both are pure byte-level parsers
//! with NO `charset.c`/`g_chartab` dependency at all, despite an
//! earlier session's own deferral note claiming otherwise (corrected
//! here after re-reading each real function body directly).
//!
//! Also translated: [`KeyvalueT`] and its four comparators
//! [`cmp_keyvalue_value`]/[`cmp_keyvalue_value_n`]/
//! [`cmp_keyvalue_value_i`]/[`cmp_keyvalue_value_ni`]. The `_n`/`_ni`
//! pair bound by the LONGER of the two lengths, not the shorter -
//! bounding by the shorter would report a prefix as equal to the
//! longer string. `length` becomes a method over the stored bytes
//! rather than a field maintained in parallel with them.
//!
//! Deferred:
//! - `vim_snprintf`/`vim_vsnprintf`/`kv_do_printf` and the whole custom
//!   positional-argument printf machinery: Rust's native `format!`/
//!   `write!` macros are the direct replacement for this (matching
//!   `printf`-style format strings is a C-specific problem this
//!   translation doesn't have), used directly at whichever call sites
//!   actually need formatted output when those are translated.
//! - Every `f_*` function (Vimscript builtins operating on `typval_T`):
//!   belongs with the eval engine, phase 5.
//! - `cmp_keyvalue_value*`: not yet reached: no caller translated yet
//!   that needs them.
//!
//! Also translated: [`strrep`] - `reverse_text` (its own file
//! neighbor in the original) was already translated in an earlier
//! session, hosted in `eval/funcs.rs` alongside the `reverse()`
//! builtin that needs it rather than here.
//!
//! Also translated: `vim_strsave_escaped` (used by `escape()`) and
//! `vim_strsave_shellescape` (used by `shellescape()`) - re-examined
//! after this module doc's own earlier note grouped both under
//! "needs `charset.c`'s real `g_chartab`/`option.c`" (stale even for
//! `vim_strsave_escaped`, translated in an earlier session).
//! `vim_strsave_shellescape` itself needs no chartab access at all -
//! only `crate::option::csh_like_shell`/`fish_like_shell`,
//! `crate::ex_docmd::find_cmdline_var`, and `crate::mbyte::utfc_ptr2len`,
//! all either already existing or newly harvested alongside.
//!
//! `vim_strsave_escaped` is now the `cc = '\\'`/`bsl = false` special
//! case of the fuller `vim_strsave_escaped_ext` (also translated) -
//! the general form's real callers (`register.c`'s `@:`-register
//! command-line escaping, `os/shell.c`'s shell-quote escaping) aren't
//! translated yet, but the function itself is small, self-contained,
//! and has no design freedom to get wrong, matching this crate's own
//! established "translate ahead of a real caller" precedent.
//! Deliberately collapses the original's own separate length-counting
//! pass and fixed-size-buffer-filling pass into a single pass building
//! a growing `Vec<u8>` - Rust's own `Vec` has no need for the
//! original's C-style pre-sizing dance (matching this crate's own
//! established `winrestcmd()`/`grow_string_tv` precedent for this
//! exact simplification).

use crate::ascii_defs::NUL;
use crate::macros_defs::tolower_loc;

/// Copy up to `len` bytes of `string` into newly allocated memory and
/// NUL-terminate. The result always has size `len + 1`, even when `string`
/// is shorter than `len` (`xstrnsave`).
///
/// Note: like the rest of this crate's memory-module conventions, `string`
/// is modeled as exact-length content, not a NUL-scanned C buffer - so
/// unlike the original's `strncpy` (which stops copying at an embedded NUL
/// and zero-pads the rest), this simply copies all of `string` (up to
/// `len` bytes) verbatim.
pub fn xstrnsave(string: &[u8], len: usize) -> Vec<u8> {
    let mut ret = vec![0u8; len + 1];
    let n = string.len().min(len);
    ret[..n].copy_from_slice(&string[..n]);
    ret
}

/// Remove quotes from `string`, unescaping a backslash-escaped `\` or
/// `"` while inside a quoted section (`vim_strnsave_unquoted`).
///
/// A pure byte-level parser - no `charset.c`/`g_chartab` dependency at
/// all (an earlier session's own deferral note claiming otherwise was
/// stale, corrected here after re-reading the real function body).
#[must_use]
pub fn vim_strnsave_unquoted(string: &[u8]) -> Vec<u8> {
    let mut ret = Vec::with_capacity(string.len());
    let mut inquote = false;
    let mut i = 0;
    while i < string.len() {
        let c = string[i];
        if c == b'"' {
            inquote = !inquote;
            i += 1;
        } else if c == b'\\'
            && inquote
            && i + 1 < string.len()
            && (string[i + 1] == b'\\' || string[i + 1] == b'"')
        {
            ret.push(string[i + 1]);
            i += 2;
        } else {
            ret.push(c);
            i += 1;
        }
    }
    ret
}

/// Compare two strings ignoring case, using the current locale
/// (`vim_stricmp`). Doesn't work for multi-byte characters.
///
/// Returns `0` for a match, `<0` if `s1 < s2`, `>0` if `s1 > s2`.
pub fn vim_stricmp(s1: &[u8], s2: &[u8]) -> i32 {
    let mut i1 = s1.iter();
    let mut i2 = s2.iter();
    loop {
        let c1 = i1.next().copied().unwrap_or(NUL);
        let c2 = i2.next().copied().unwrap_or(NUL);
        let diff = tolower_loc(c1 as i32) - tolower_loc(c2 as i32);
        if diff != 0 {
            return diff; // this character different
        }
        if c1 == NUL {
            break; // strings match until NUL
        }
    }
    0 // strings match
}

/// Compare two strings for length `len`, ignoring case, using the current
/// locale (`vim_strnicmp`). Doesn't work for multi-byte characters.
///
/// Returns `0` for a match, `<0` if `s1 < s2`, `>0` if `s1 > s2`.
pub fn vim_strnicmp(s1: &[u8], s2: &[u8], len: usize) -> i32 {
    let mut i1 = s1.iter();
    let mut i2 = s2.iter();
    for _ in 0..len {
        let c1 = i1.next().copied().unwrap_or(NUL);
        let c2 = i2.next().copied().unwrap_or(NUL);
        let diff = tolower_loc(c1 as i32) - tolower_loc(c2 as i32);
        if diff != 0 {
            return diff; // this character different
        }
        if c1 == NUL {
            break; // strings match until NUL
        }
    }
    0 // strings match
}

/// Case-insensitive [`crate::memory::strequal`] (`striequal`).
pub fn striequal(a: Option<&[u8]>, b: Option<&[u8]>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => vim_stricmp(a, b) == 0,
        _ => false,
    }
}

/// Compare two ASCII strings for length `len`, ignoring case, ignoring
/// locale (`vim_strnicmp_asc`).
///
/// Returns `0` for a match, `<0` if `s1 < s2`, `>0` if `s1 > s2`.
pub fn vim_strnicmp_asc(s1: &[u8], s2: &[u8], len: usize) -> i32 {
    use crate::macros_defs::tolower_asc;
    let mut i1 = s1.iter();
    let mut i2 = s2.iter();
    let mut i = 0;
    for _ in 0..len {
        let c1 = i1.next().copied().unwrap_or(NUL);
        let c2 = i2.next().copied().unwrap_or(NUL);
        i = tolower_asc(c1 as i32) - tolower_asc(c2 as i32);
        if i != 0 {
            break; // this character is different
        }
        if c1 == NUL {
            break; // strings match until NUL
        }
    }
    i
}

/// A key/value pair (`keyvalue_T`), used for the small lookup tables
/// this file's comparators sort and search.
///
/// The original keeps `length` alongside `value` because `value` is a
/// bare `char *`; here it is derived from the stored bytes, so the two
/// cannot disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyvalueT {
    /// the key
    pub key: i32,
    /// the value string
    pub value: Vec<u8>,
}

impl KeyvalueT {
    /// `KEYVALUE_ENTRY(k, v)`
    #[must_use]
    pub fn new(key: i32, value: &[u8]) -> Self {
        Self { key, value: value.to_vec() }
    }

    /// Length of the value string (`length`).
    #[must_use]
    pub fn length(&self) -> usize {
        self.value.len()
    }
}

/// Compare two [`KeyvalueT`]s by value (`cmp_keyvalue_value`).
///
/// Returns [`std::cmp::Ordering`] rather than a C comparator's
/// negative/zero/positive `int`, so it drops straight into `sort_by`.
#[must_use]
pub fn cmp_keyvalue_value(kv1: &KeyvalueT, kv2: &KeyvalueT) -> std::cmp::Ordering {
    kv1.value.cmp(&kv2.value)
}

/// Compare two [`KeyvalueT`]s by value, bounded by the LONGER of the
/// two lengths (`cmp_keyvalue_value_n`).
///
/// Note the bound is `MAX`, not `MIN`: comparing only the shorter
/// length would report a prefix as equal to the longer string, so the
/// longer one is what decides.
#[must_use]
pub fn cmp_keyvalue_value_n(kv1: &KeyvalueT, kv2: &KeyvalueT) -> std::cmp::Ordering {
    let n = kv1.length().max(kv2.length());
    strncmp_bytes(&kv1.value, &kv2.value, n).cmp(&0)
}

/// Compare two [`KeyvalueT`]s by value, ignoring case
/// (`cmp_keyvalue_value_i`).
#[must_use]
pub fn cmp_keyvalue_value_i(kv1: &KeyvalueT, kv2: &KeyvalueT) -> std::cmp::Ordering {
    vim_stricmp(&kv1.value, &kv2.value).cmp(&0)
}

/// Compare two [`KeyvalueT`]s by value, ignoring case and bounded by
/// the LONGER of the two lengths (`cmp_keyvalue_value_ni`).
#[must_use]
pub fn cmp_keyvalue_value_ni(kv1: &KeyvalueT, kv2: &KeyvalueT) -> std::cmp::Ordering {
    let n = kv1.length().max(kv2.length());
    vim_strnicmp(&kv1.value, &kv2.value, n).cmp(&0)
}

/// `strncmp` over byte slices, treating a byte past either slice's own
/// end as the NUL a real C string would have there.
fn strncmp_bytes(s1: &[u8], s2: &[u8], n: usize) -> i32 {
    for i in 0..n {
        let c1 = s1.get(i).copied().unwrap_or(0);
        let c2 = s2.get(i).copied().unwrap_or(0);
        if c1 != c2 {
            return i32::from(c1) - i32::from(c2);
        }
        if c1 == 0 {
            break;
        }
    }
    0
}

/// Sort an array of strings (`sort_strings`). The original sorts in place
/// via `qsort`+`strcmp`; `sort_unstable` is Rust's native equivalent
/// (`strcmp`-style byte-lexicographic ordering is exactly `Ord` for
/// `[u8]`/`Vec<u8>`, and `qsort` never claimed stability either).
pub fn sort_strings(files: &mut [Vec<u8>]) {
    files.sort_unstable();
}

/// Returns true if `s` contains a non-ASCII byte (128 or higher)
/// (`has_non_ascii`/`has_non_ascii_len` - unified, since `&[u8]` always
/// carries its own length; `None`/absent input returns false like the
/// original's `NULL` case).
pub fn has_non_ascii(s: Option<&[u8]>) -> bool {
    match s {
        Some(s) => s.iter().any(|&b| b >= 128),
        None => false,
    }
}

/// Return the length of `s` with trailing whitespace removed, unless
/// it's escaped by a preceding `\` or Ctrl-V (`del_trailing_spaces`).
///
/// The original mutates a NUL-terminated C string in place, writing
/// `NUL` bytes at the very end to shrink it; since a `&[u8]` slice
/// already carries its own exact length (no NUL terminator needed),
/// this returns the new, shorter length instead of writing anything -
/// a caller wanting an in-place shrink can
/// `s.truncate(del_trailing_spaces_len(&s))`. Always keeps at least
/// the first byte (matching the original's own `q > ptr` bound, which
/// never lets `q` reach the very first character).
#[must_use]
pub fn del_trailing_spaces_len(s: &[u8]) -> usize {
    let mut len = s.len();
    let mut q = s.len();
    loop {
        if q == 0 {
            break;
        }
        q -= 1;
        if q == 0 || !crate::ascii_defs::ascii_iswhite(i32::from(s[q])) {
            break;
        }
        let prev = s[q - 1];
        if prev == b'\\' || prev == crate::ascii_defs::CTRL_V {
            break;
        }
        len = q;
    }
    len
}

/// Concatenate two strings and return the result in newly allocated memory
/// (`concat_str`).
pub fn concat_str(str1: &[u8], str2: &[u8]) -> Vec<u8> {
    let mut dest = Vec::with_capacity(str1.len() + str2.len());
    dest.extend_from_slice(str1);
    dest.extend_from_slice(str2);
    dest
}

/// ASCII lower-to-upper case translation, language independent, in
/// place (`vim_strup`).
///
/// Stops at the first embedded NUL byte, matching the original's own
/// `while ((c = *p) != NUL)` loop exactly (a genuine NUL-terminated C
/// string never has meaningful content past its first NUL) - anything
/// in `p` from that point on is left untouched, not uppercased.
pub fn vim_strup(p: &mut [u8]) {
    for c in p.iter_mut() {
        if *c == NUL {
            break;
        }
        if c.is_ascii_lowercase() {
            *c -= 0x20;
        }
    }
}

/// Copy a NUL-terminated string while uppercasing ASCII letters
/// (`vim_strcpy_up`).
pub fn vim_strcpy_up(destination: &mut [u8], source: &[u8]) {
    let mut length = 0;
    for &byte in source {
        if byte == NUL {
            break;
        }
        destination[length] = byte.to_ascii_uppercase();
        length += 1;
    }
    destination[length] = NUL;
}

/// Like [`xstrnsave`], but make all characters uppercase using ASCII
/// lower-to-upper case translation, language independent
/// (`vim_strsave_up`).
///
/// Unlike [`xstrnsave`] (a lower-level "copy exactly N bytes, embedded
/// NULs included" primitive taking an explicit length), this - like
/// [`vim_strup`] - is self-bounding via NUL-scanning, matching the
/// original's own `strlen(string)`-sized allocation: the result is
/// truncated at `string`'s first embedded NUL (if any), then
/// NUL-terminated there, not a verbatim same-length copy.
#[must_use]
pub fn vim_strsave_up(string: &[u8]) -> Vec<u8> {
    let end = string.iter().position(|&b| b == NUL).unwrap_or(string.len());
    let mut result = string[..end].to_vec();
    vim_strup(&mut result);
    result.push(NUL);
    result
}

/// Multi-byte uppercase `src`, returning a newly allocated result
/// (`mb_strup_buf`).
///
/// Deviates from the original's `char *dst` out-parameter (which the
/// caller must pre-size to `strlen(src) * MB_MAXBYTES + 1` in the
/// worst case): returns a freshly, exactly sized `Vec<u8>` instead,
/// sidestepping that sizing concern entirely. Matches the original's
/// own explicit NUL-termination (`dst[i] = NUL;`): the returned
/// `Vec<u8>` includes a trailing NUL byte, same as this crate's other
/// `strings.c` functions (e.g. [`xstrnsave`]) and the original's own
/// NUL-terminated-C-string representation.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via
/// [`crate::mbyte::mb_toupper`]) - same requirement as every other
/// function that does so.
#[must_use]
pub unsafe fn mb_strup_buf(src: &[u8]) -> Vec<u8> {
    let mut dst = Vec::with_capacity(src.len() + 1);
    let mut p = 0usize;
    while p < src.len() && src[p] != NUL {
        let ci = crate::mbyte::utf_ptr2char_info(&src[p..]);
        let c = if ci.value < 0 { i32::from(src[p]) } else { ci.value };
        // SAFETY: forwarded from this function's own safety doc.
        let upper = unsafe { crate::mbyte::mb_toupper(c) };
        let mut buf = [0u8; crate::mbyte_defs::MB_MAXBYTES];
        let n = crate::mbyte::utf_char2bytes(upper, &mut buf) as usize;
        dst.extend_from_slice(&buf[..n]);
        p += ci.len;
    }
    dst.push(NUL);
    dst
}

/// Make given string all upper-case or all lower-case, returning a
/// newly allocated result (`strcase_save`).
///
/// Handles multi-byte characters as good as possible. Matches the
/// original's own explicit NUL-termination (`res[res_index] = NUL;`):
/// the returned `Vec<u8>` includes a trailing NUL byte, same as
/// [`mb_strup_buf`] above.
///
/// @param upper If true make uppercase, otherwise lowercase.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via
/// [`crate::mbyte::mb_toupper`]/[`crate::mbyte::mb_tolower`]).
#[must_use]
pub unsafe fn strcase_save(orig: &[u8], upper: bool) -> Vec<u8> {
    let mut res = Vec::with_capacity(orig.len() + 1);
    let mut p = 0usize;
    while p < orig.len() && orig[p] != NUL {
        let char_info = crate::mbyte::utf_ptr2char_info(&orig[p..]);
        let c = if char_info.value < 0 { i32::from(orig[p]) } else { char_info.value };
        // SAFETY: forwarded from this function's own safety doc.
        let newc = unsafe { if upper { crate::mbyte::mb_toupper(c) } else { crate::mbyte::mb_tolower(c) } };

        let mut buf = [0u8; crate::mbyte_defs::MB_MAXBYTES];
        let newl = crate::mbyte::utf_char2bytes(newc, &mut buf) as usize;
        res.extend_from_slice(&buf[..newl]);
        p += char_info.len;
    }
    res.push(NUL);
    res
}

/// `strchr()` version which handles multibyte strings (`vim_strchr`).
///
/// @param string  String to search in.
/// @param c  Character to search for.
///
/// @return the byte offset of the first occurrence of character `c` in
/// `string`, or `None` if it was not found or the character is invalid.
/// The NUL character is never found (matching the original's own
/// documented caveat - use `.len()` instead), and the scan never looks
/// past the first embedded NUL (matching the original's own
/// NUL-terminated-C-string `strchr`/`strstr` semantics, since a Rust
/// `&[u8]` has no implicit terminator of its own).
#[must_use]
pub fn vim_strchr(string: &[u8], c: i32) -> Option<usize> {
    if c <= 0 {
        return None;
    }

    let end = string.iter().position(|&b| b == NUL).unwrap_or(string.len());
    let string = &string[..end];

    if c < 0x80 {
        return string.iter().position(|&b| b == c as u8);
    }

    let mut u8char = [0u8; crate::mbyte_defs::MB_MAXCHAR];
    let len = crate::mbyte::utf_char2bytes(c, &mut u8char) as usize;
    let needle = &u8char[..len];
    if needle.is_empty() || needle.len() > string.len() {
        return None;
    }
    string.windows(needle.len()).position(|w| w == needle)
}

/// Replace all occurrences of `what` with `rep` in `src`. Returns
/// `None` if no replacement happens, matching the original's `NULL`
/// return when there's nothing to replace (`strrep`).
///
/// A genuinely empty `what` would make the original's own `strstr`-
/// based loop spin forever (an empty needle "matches" at every
/// position without ever advancing `pos`) - not reachable from this
/// function's real caller (`ex_docmd.c`'s `:redir`, not yet
/// translated), so this is guarded explicitly here as a documented
/// precondition instead of literally reproducing the hang.
#[must_use]
pub fn strrep(src: &[u8], what: &[u8], rep: &[u8]) -> Option<Vec<u8>> {
    if what.is_empty() {
        return None;
    }

    let mut found_any = false;
    let mut ret = Vec::with_capacity(src.len());
    let mut remaining = src;
    while let Some(pos) = remaining.windows(what.len()).position(|w| w == what) {
        found_any = true;
        ret.extend_from_slice(&remaining[..pos]);
        ret.extend_from_slice(rep);
        remaining = &remaining[pos + what.len()..];
    }

    if !found_any {
        return None;
    }
    ret.extend_from_slice(remaining);
    Some(ret)
}

/// Escape every character in `string` that also appears in
/// `esc_chars`, escaping with `cc`; when `bsl` is `true`, ALSO escape
/// characters where [`crate::charset::rem_backslash`] would remove
/// the backslash (`vim_strsave_escaped_ext`). [`vim_strsave_escaped`]
/// is the `cc = '\\'`/`bsl = false` special case this crate's own
/// earlier translation of it already hardcoded - this general form
/// exists to serve `register.c`'s `@:`-register command-line
/// escaping and `os/shell.c`'s shell-quote escaping, once either of
/// those files' own real call sites are translated.
///
/// # Safety
/// Touches `OPTION_VARS` (via [`crate::mbyte::utfc_ptr2len`]).
#[must_use]
pub unsafe fn vim_strsave_escaped_ext(string: &[u8], esc_chars: &[u8], cc: u8, bsl: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(string.len());
    let mut p = 0usize;
    while p < string.len() {
        // SAFETY: forwarded from this function's own safety doc.
        let l = usize::try_from(unsafe { crate::mbyte::utfc_ptr2len(&string[p..]) }).unwrap_or(0);
        if l > 1 {
            out.extend_from_slice(&string[p..p + l]);
            p += l;
            continue;
        }
        if vim_strchr(esc_chars, i32::from(string[p])).is_some()
            || (bsl && crate::charset::rem_backslash(&string[p..]))
        {
            out.push(cc);
        }
        out.push(string[p]);
        p += 1;
    }
    out
}

/// Escape every character in `string` that also appears in
/// `esc_chars` with a backslash (`vim_strsave_escaped`, the
/// `cc = '\\'`/`bsl = false` special case of
/// [`vim_strsave_escaped_ext`]).
///
/// # Safety
/// Forwards [`vim_strsave_escaped_ext`]'s own safety requirement.
#[must_use]
pub unsafe fn vim_strsave_escaped(string: &[u8], esc_chars: &[u8]) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { vim_strsave_escaped_ext(string, esc_chars, b'\\', false) }
}

/// Escape `string` for use as a shell command-line argument, wrapping
/// it in quotes (`'`...`'`, or `"`...`"` on Windows without
/// `'shellslash'` set) and escaping embedded quotes/special characters
/// as needed (`vim_strsave_shellescape`). `do_special` additionally
/// escapes `'!'` and cmdline-special-variable sequences (`%`, `#`,
/// `<cword>`, etc. - see [`crate::ex_docmd::find_cmdline_var`]);
/// `do_newline` additionally escapes embedded newlines. csh-like
/// shells escape both twice (once for Nvim, once for the shell
/// itself, since csh's own single-quote handling still expands `!`);
/// fish-like shells additionally escape a literal backslash (fish
/// treats `\` as an escape character even within single quotes).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via
/// [`crate::mbyte::utfc_ptr2len`], and, on Windows, directly for
/// `'shellslash'`).
#[must_use]
pub unsafe fn vim_strsave_shellescape(string: &[u8], do_special: bool, do_newline: bool) -> Vec<u8> {
    let csh_like = crate::option::csh_like_shell();
    let fish_like = crate::option::fish_like_shell();

    // On Windows, without 'shellslash', the whole string is wrapped in
    // double quotes with "" escaping an embedded double quote
    // (cmd.exe's own quoting convention); everywhere else - including
    // Windows WITH 'shellslash' set - it's wrapped in single quotes
    // with '\'' escaping an embedded single quote (POSIX shell
    // quoting). `use_dquote` is unconditionally `false` on non-Windows
    // targets, matching the original's own `#ifdef MSWIN` guard.
    #[cfg(windows)]
    // SAFETY: forwarded from this function's own safety doc.
    let use_dquote = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ssl == 0;
    #[cfg(not(windows))]
    let use_dquote = false;
    let quote = if use_dquote { b'"' } else { b'\'' };

    let mut out = Vec::with_capacity(string.len() + 2);
    out.push(quote);

    let mut p = 0usize;
    while p < string.len() {
        let c = string[p];
        if use_dquote && c == b'"' {
            out.push(b'"');
            out.push(b'"');
            p += 1;
            continue;
        }
        if !use_dquote && c == b'\'' {
            out.extend_from_slice(b"'\\''");
            p += 1;
            continue;
        }
        if (c == b'\n' && (csh_like || do_newline)) || (c == b'!' && (csh_like || do_special)) {
            out.push(b'\\');
            if csh_like && do_special {
                out.push(b'\\');
            }
            out.push(c);
            p += 1;
            continue;
        }
        if do_special
            && let Some((_, used_len)) = crate::ex_docmd::find_cmdline_var(&string[p..])
        {
            out.push(b'\\'); // insert backslash
            out.extend_from_slice(&string[p..p + used_len]);
            p += used_len;
            continue;
        }
        if c == b'\\' && fish_like {
            out.push(b'\\');
            out.push(c);
            p += 1;
            continue;
        }
        // mb_copy_char: copy one full (possibly multi-byte,
        // composing-character-inclusive) character.
        // SAFETY: forwarded from this function's own safety doc.
        let l = usize::try_from(unsafe { crate::mbyte::utfc_ptr2len(&string[p..]) }).unwrap_or(1).max(1);
        let l = l.min(string.len() - p);
        out.extend_from_slice(&string[p..p + l]);
        p += l;
    }

    out.push(quote);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmp_keyvalue_value_orders_by_the_value_bytes() {
        use std::cmp::Ordering;
        let a = KeyvalueT::new(1, b"apple");
        let b = KeyvalueT::new(2, b"banana");
        assert_eq!(cmp_keyvalue_value(&a, &b), Ordering::Less);
        assert_eq!(cmp_keyvalue_value(&b, &a), Ordering::Greater);
        assert_eq!(cmp_keyvalue_value(&a, &a.clone()), Ordering::Equal);
        // The key plays no part in the ordering.
        assert_eq!(
            cmp_keyvalue_value(&KeyvalueT::new(99, b"apple"), &a),
            Ordering::Equal
        );
    }

    #[test]
    fn cmp_keyvalue_value_n_bounds_by_the_longer_length() {
        use std::cmp::Ordering;
        // The bound is MAX, not MIN: bounding by the shorter length
        // would call a prefix equal to the longer string.
        let short = KeyvalueT::new(1, b"ab");
        let long = KeyvalueT::new(2, b"abc");
        assert_eq!(cmp_keyvalue_value_n(&short, &long), Ordering::Less);
        assert_eq!(cmp_keyvalue_value_n(&long, &short), Ordering::Greater);
        assert_eq!(cmp_keyvalue_value_n(&long, &long.clone()), Ordering::Equal);
    }

    #[test]
    fn cmp_keyvalue_value_i_ignores_case() {
        use std::cmp::Ordering;
        let lower = KeyvalueT::new(1, b"abc");
        let upper = KeyvalueT::new(2, b"ABC");
        assert_eq!(cmp_keyvalue_value_i(&lower, &upper), Ordering::Equal);
        // ...but a genuine difference still orders.
        assert_eq!(
            cmp_keyvalue_value_i(&lower, &KeyvalueT::new(3, b"ABD")),
            Ordering::Less
        );
    }

    #[test]
    fn cmp_keyvalue_value_ni_ignores_case_and_bounds_by_the_longer() {
        use std::cmp::Ordering;
        let short = KeyvalueT::new(1, b"AB");
        let long = KeyvalueT::new(2, b"abc");
        assert_eq!(cmp_keyvalue_value_ni(&short, &long), Ordering::Less);
        assert_eq!(
            cmp_keyvalue_value_ni(&KeyvalueT::new(1, b"ABC"), &long),
            Ordering::Equal
        );
    }

    #[test]
    fn keyvalue_length_tracks_the_stored_value() {
        // The original carries `length` as its own field; here it is
        // derived, so it cannot disagree with the value.
        assert_eq!(KeyvalueT::new(1, b"abcd").length(), 4);
        assert_eq!(KeyvalueT::new(1, b"").length(), 0);
    }

    #[test]
    fn cmp_keyvalue_value_sorts_a_table() {
        let mut table = [
            KeyvalueT::new(1, b"cherry"),
            KeyvalueT::new(2, b"apple"),
            KeyvalueT::new(3, b"banana"),
        ];
        table.sort_by(cmp_keyvalue_value);
        let values: Vec<&[u8]> = table.iter().map(|kv| kv.value.as_slice()).collect();
        assert_eq!(values, vec![&b"apple"[..], &b"banana"[..], &b"cherry"[..]]);
    }

    #[test]
    fn xstrnsave_pads_short_strings_and_truncates_long_ones() {
        let v = xstrnsave(b"ab", 5);
        assert_eq!(v.len(), 6);
        assert_eq!(&v[..2], b"ab");
        assert!(v[2..].iter().all(|&b| b == 0));

        let v2 = xstrnsave(b"abcdef", 3);
        assert_eq!(v2.len(), 4);
        assert_eq!(&v2[..3], b"abc");
    }

    #[test]
    fn vim_strnsave_unquoted_strips_surrounding_quotes() {
        assert_eq!(vim_strnsave_unquoted(b"\"hello\""), b"hello".to_vec());
    }

    #[test]
    fn vim_strnsave_unquoted_unescapes_an_escaped_quote() {
        assert_eq!(vim_strnsave_unquoted(b"\"a\\\"b\""), b"a\"b".to_vec());
    }

    #[test]
    fn vim_strnsave_unquoted_unescapes_an_escaped_backslash() {
        assert_eq!(vim_strnsave_unquoted(b"\"a\\\\b\""), b"a\\b".to_vec());
    }

    #[test]
    fn vim_strnsave_unquoted_leaves_an_unquoted_string_untouched() {
        // Outside quotes, a backslash is just a plain character - the
        // escape condition only fires while `inquote` is true.
        assert_eq!(vim_strnsave_unquoted(b"a\\b"), b"a\\b".to_vec());
    }

    #[test]
    fn vim_strnsave_unquoted_only_unescapes_backslash_and_quote() {
        // "\n" (backslash followed by a plain 'n', not \\ or ") does
        // NOT match the escape condition even while inquote - the
        // backslash is kept as a literal character.
        assert_eq!(vim_strnsave_unquoted(b"\"a\\nb\""), b"a\\nb".to_vec());
    }

    #[test]
    fn del_trailing_spaces_len_strips_trailing_whitespace() {
        assert_eq!(del_trailing_spaces_len(b"hello   "), 5);
    }

    #[test]
    fn del_trailing_spaces_len_unchanged_when_no_trailing_whitespace() {
        assert_eq!(del_trailing_spaces_len(b"hello"), 5);
    }

    #[test]
    fn del_trailing_spaces_len_keeps_a_space_escaped_by_backslash() {
        assert_eq!(del_trailing_spaces_len(b"a\\ "), 3);
    }

    #[test]
    fn del_trailing_spaces_len_keeps_a_space_escaped_by_ctrl_v() {
        assert_eq!(
            del_trailing_spaces_len(&[b'a', crate::ascii_defs::CTRL_V, b' ']),
            3
        );
    }

    #[test]
    fn del_trailing_spaces_len_all_whitespace_keeps_the_first_byte() {
        assert_eq!(del_trailing_spaces_len(b"   "), 1);
    }

    #[test]
    fn del_trailing_spaces_len_empty_string_is_zero() {
        assert_eq!(del_trailing_spaces_len(b""), 0);
    }

    #[test]
    fn del_trailing_spaces_len_single_space_is_unchanged() {
        assert_eq!(del_trailing_spaces_len(b" "), 1);
    }

    #[test]
    fn vim_stricmp_ignores_case() {
        assert_eq!(vim_stricmp(b"Hello", b"hello"), 0);
        assert_ne!(vim_stricmp(b"Hello", b"World"), 0);
        assert_eq!(vim_stricmp(b"abc", b"abc"), 0);
    }

    #[test]
    fn vim_strnicmp_bounds_by_len() {
        assert_eq!(vim_strnicmp(b"ABCxyz", b"abcXYZ", 3), 0); // "ABC" vs "abc" ci-equal within len=3
        assert_eq!(vim_strnicmp(b"ABCxyz", b"abcXYZ", 6), 0); // full ci-equal too
        assert_ne!(vim_strnicmp(b"ABCabc", b"ABCxyz", 6), 0); // genuinely differs
    }

    #[test]
    fn striequal_handles_none_like_strequal() {
        assert!(striequal(None, None));
        assert!(!striequal(None, Some(b"a")));
        assert!(striequal(Some(b"ABC"), Some(b"abc")));
    }

    #[test]
    fn vim_strnicmp_asc_is_locale_independent() {
        assert_eq!(vim_strnicmp_asc(b"ABC", b"abc", 3), 0);
    }

    #[test]
    fn sort_strings_sorts_lexicographically() {
        let mut v = vec![b"banana".to_vec(), b"apple".to_vec(), b"cherry".to_vec()];
        sort_strings(&mut v);
        assert_eq!(v, vec![b"apple".to_vec(), b"banana".to_vec(), b"cherry".to_vec()]);
    }

    #[test]
    fn has_non_ascii_detects_high_bytes() {
        assert!(!has_non_ascii(Some(b"hello")));
        assert!(has_non_ascii(Some(&[b'h', 200, b'i'])));
        assert!(!has_non_ascii(None));
    }

    #[test]
    fn concat_str_joins_without_separator() {
        assert_eq!(concat_str(b"foo", b"bar"), b"foobar");
    }

    #[test]
    fn vim_strup_uppercases_ascii_letters_in_place() {
        let mut s = b"Hello, World! 123\0".to_vec();
        vim_strup(&mut s);
        assert_eq!(&s, b"HELLO, WORLD! 123\0");
    }

    #[test]
    fn vim_strup_stops_at_first_embedded_nul() {
        let mut s = b"ab\0cd".to_vec(); // 'c'/'d' come after an embedded NUL
        vim_strup(&mut s);
        assert_eq!(&s, b"AB\0cd"); // untouched past the NUL
    }

    #[test]
    fn vim_strcpy_up_copies_uppercase_through_the_first_nul() {
        let mut destination = [0xaa; 8];
        vim_strcpy_up(&mut destination, b"abC\0ignored");
        assert_eq!(&destination[..4], b"ABC\0");
        assert_eq!(&destination[4..], &[0xaa; 4]);
    }

    #[test]
    fn vim_strsave_up_returns_nul_terminated_uppercase_copy() {
        assert_eq!(vim_strsave_up(b"hello\0"), b"HELLO\0");
    }

    #[test]
    fn vim_strsave_up_truncates_at_first_embedded_nul() {
        // Matches the original's own strlen()-based sizing: content
        // past the first NUL isn't part of the "real" string at all,
        // so the result is truncated there (not just left unmodified).
        assert_eq!(vim_strsave_up(b"ab\0cd"), b"AB\0");
    }

    #[test]
    fn mb_strup_buf_uppercases_ascii_and_multibyte() {
        let _guard = crate::globals::global_state_test_lock();
        // SAFETY: touches OPTION_VARS via mb_toupper, guarded above.
        let result = unsafe { mb_strup_buf("héllo\0".as_bytes()) };
        assert_eq!(result, "HÉLLO\0".as_bytes());
    }

    #[test]
    fn mb_strup_buf_stops_at_first_embedded_nul() {
        let _guard = crate::globals::global_state_test_lock();
        // SAFETY: forwarded, guarded above.
        let result = unsafe { mb_strup_buf(b"ab\0cd") };
        assert_eq!(result, b"AB\0");
    }

    #[test]
    fn strcase_save_uppercases_when_requested() {
        let _guard = crate::globals::global_state_test_lock();
        // SAFETY: touches OPTION_VARS via mb_toupper, guarded above.
        let result = unsafe { strcase_save("héllo\0".as_bytes(), true) };
        assert_eq!(result, "HÉLLO\0".as_bytes());
    }

    #[test]
    fn strcase_save_lowercases_when_requested() {
        let _guard = crate::globals::global_state_test_lock();
        // SAFETY: touches OPTION_VARS via mb_tolower, guarded above.
        let result = unsafe { strcase_save("HÉLLO\0".as_bytes(), false) };
        assert_eq!(result, "héllo\0".as_bytes());
    }

    #[test]
    fn vim_strchr_finds_ascii_byte() {
        assert_eq!(vim_strchr(b"hello\0", i32::from(b'l')), Some(2));
    }

    #[test]
    fn vim_strchr_not_found_returns_none() {
        assert_eq!(vim_strchr(b"hello\0", i32::from(b'z')), None);
    }

    #[test]
    fn vim_strchr_never_finds_nul() {
        assert_eq!(vim_strchr(b"hello\0", 0), None);
    }

    #[test]
    fn vim_strchr_rejects_negative_c() {
        assert_eq!(vim_strchr(b"hello\0", -1), None);
    }

    #[test]
    fn vim_strchr_stops_at_first_embedded_nul() {
        // "z" only appears after the embedded NUL - matching real
        // strchr()'s own NUL-terminated-string semantics, it must not
        // be found.
        assert_eq!(vim_strchr(b"ab\0z", i32::from(b'z')), None);
    }

    #[test]
    fn vim_strchr_finds_multibyte_character() {
        // "héllo\0": h=1 byte, é=2 bytes (U+00E9), so 'é' starts at
        // byte offset 1.
        assert_eq!(vim_strchr("héllo\0".as_bytes(), 0xe9), Some(1));
    }

    #[test]
    fn vim_strchr_multibyte_not_found() {
        assert_eq!(vim_strchr("hello\0".as_bytes(), 0xe9), None);
    }

    #[test]
    fn strrep_replaces_a_single_occurrence() {
        assert_eq!(
            strrep(b"hello world", b"world", b"there"),
            Some(b"hello there".to_vec())
        );
    }

    #[test]
    fn strrep_replaces_all_non_overlapping_occurrences() {
        assert_eq!(strrep(b"aaa", b"a", b"bb"), Some(b"bbbbbb".to_vec()));
    }

    #[test]
    fn strrep_none_when_not_found() {
        assert_eq!(strrep(b"hello", b"xyz", b"abc"), None);
    }

    #[test]
    fn strrep_none_on_empty_source() {
        assert_eq!(strrep(b"", b"a", b"b"), None);
    }

    #[test]
    fn strrep_none_on_empty_what() {
        assert_eq!(strrep(b"hello", b"", b"x"), None);
    }

    #[test]
    fn strrep_matches_are_non_overlapping() {
        // "aa" matches once at position 0 in "aaa" (consuming both a's),
        // leaving only the last "a" - which is too short for a second
        // "aa" match.
        assert_eq!(strrep(b"aaa", b"aa", b"b"), Some(b"ba".to_vec()));
    }

    #[test]
    fn vim_strsave_escaped_escapes_matching_chars() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { vim_strsave_escaped(b"a b c", b" ") }, b"a\\ b\\ c".to_vec());
    }

    #[test]
    fn vim_strsave_escaped_no_matching_chars_is_unchanged() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { vim_strsave_escaped(b"hello", b" ") }, b"hello".to_vec());
    }

    #[test]
    fn vim_strsave_escaped_empty_string_is_empty() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { vim_strsave_escaped(b"", b" ") }, Vec::<u8>::new());
    }

    #[test]
    fn vim_strsave_escaped_escapes_every_occurrence() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { vim_strsave_escaped(b"a.b.c", b".") }, b"a\\.b\\.c".to_vec());
    }

    #[test]
    fn vim_strsave_escaped_multiple_esc_chars() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { vim_strsave_escaped(b"a b.c", b" .") }, b"a\\ b\\.c".to_vec());
    }

    #[test]
    fn vim_strsave_escaped_preserves_multibyte_characters_unescaped() {
        let _guard = crate::globals::global_state_test_lock();
        // A multibyte character is never itself a candidate for
        // escaping (matching the original's own `if (l > 1) { ...
        // continue; }` fast path skipping the esc_chars check
        // entirely for multibyte sequences).
        assert_eq!(unsafe { vim_strsave_escaped("a一b".as_bytes(), "一".as_bytes()) }, "a一b".as_bytes().to_vec());
    }

    #[test]
    fn vim_strsave_escaped_ext_uses_a_custom_escape_character() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(
            unsafe { vim_strsave_escaped_ext(b"a b", b" ", b'^', false) },
            b"a^ b".to_vec()
        );
    }

    #[test]
    fn vim_strsave_escaped_ext_bsl_also_escapes_rem_backslash_candidates() {
        // On non-Windows, rem_backslash accepts every backslash not
        // at the very end of the string - with bsl=true, THAT
        // backslash gets an extra escape character prepended too,
        // even though '\\' itself isn't in esc_chars.
        let _guard = crate::globals::global_state_test_lock();
        if cfg!(unix) {
            assert_eq!(
                unsafe { vim_strsave_escaped_ext(b"a\\b", b"", b'^', true) },
                b"a^\\b".to_vec()
            );
        }
    }

    #[test]
    fn vim_strsave_escaped_ext_bsl_false_never_touches_a_lone_backslash() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(
            unsafe { vim_strsave_escaped_ext(b"a\\b", b"", b'^', false) },
            b"a\\b".to_vec()
        );
    }

    #[test]
    fn vim_strsave_escaped_ext_matches_vim_strsave_escaped_with_default_args() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(
            unsafe { vim_strsave_escaped_ext(b"a b.c", b" .", b'\\', false) },
            unsafe { vim_strsave_escaped(b"a b.c", b" .") }
        );
    }

    // --- vim_strsave_shellescape ---

    /// RAII guard forcing `'shell'`/`'shellslash'` to known values for
    /// its whole lifetime, restoring the previous values on drop (even
    /// on panic). Callers must hold `global_state_test_lock()`.
    struct ShellVarsGuard {
        prev_sh: Option<Vec<u8>>,
        prev_ssl: i32,
    }

    impl ShellVarsGuard {
        /// A plain (non-csh, non-fish) shell, single-quote escaping
        /// (matching the original's own non-Windows default, and
        /// Windows WITH `'shellslash'` set).
        fn plain_single_quote() -> Self {
            let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let guard = ShellVarsGuard { prev_sh: ov.p_sh.clone(), prev_ssl: ov.p_ssl };
            ov.p_sh = Some(b"/bin/sh".to_vec());
            ov.p_ssl = 1;
            guard
        }

        fn set_shell(name: &[u8]) -> Self {
            let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let guard = ShellVarsGuard { prev_sh: ov.p_sh.clone(), prev_ssl: ov.p_ssl };
            ov.p_sh = Some(name.to_vec());
            ov.p_ssl = 1;
            guard
        }

        #[cfg(windows)]
        fn windows_double_quote() -> Self {
            let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let guard = ShellVarsGuard { prev_sh: ov.p_sh.clone(), prev_ssl: ov.p_ssl };
            ov.p_sh = Some(b"cmd.exe".to_vec());
            ov.p_ssl = 0;
            guard
        }
    }

    impl Drop for ShellVarsGuard {
        fn drop(&mut self) {
            let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            ov.p_sh = self.prev_sh.take();
            ov.p_ssl = self.prev_ssl;
        }
    }

    #[test]
    fn shellescape_wraps_plain_string_in_single_quotes() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShellVarsGuard::plain_single_quote();
        assert_eq!(unsafe { vim_strsave_shellescape(b"hello", false, false) }, b"'hello'".to_vec());
    }

    #[test]
    fn shellescape_escapes_embedded_single_quote() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShellVarsGuard::plain_single_quote();
        assert_eq!(unsafe { vim_strsave_shellescape(b"it's", false, false) }, b"'it'\\''s'".to_vec());
    }

    #[test]
    fn shellescape_do_special_false_leaves_bang_unescaped_for_plain_shell() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShellVarsGuard::plain_single_quote();
        assert_eq!(unsafe { vim_strsave_shellescape(b"abc!", false, false) }, b"'abc!'".to_vec());
    }

    #[test]
    fn shellescape_do_special_true_escapes_bang() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShellVarsGuard::plain_single_quote();
        assert_eq!(unsafe { vim_strsave_shellescape(b"abc!", true, true) }, b"'abc\\!'".to_vec());
    }

    #[test]
    fn shellescape_do_newline_escapes_embedded_newline() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShellVarsGuard::plain_single_quote();
        assert_eq!(unsafe { vim_strsave_shellescape(b"a\nb", false, true) }, b"'a\\\nb'".to_vec());
    }

    #[test]
    fn shellescape_no_do_newline_leaves_newline_unescaped_for_plain_shell() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShellVarsGuard::plain_single_quote();
        assert_eq!(unsafe { vim_strsave_shellescape(b"a\nb", false, false) }, b"'a\nb'".to_vec());
    }

    #[test]
    fn shellescape_do_special_escapes_cmdline_special_vars() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShellVarsGuard::plain_single_quote();
        assert_eq!(unsafe { vim_strsave_shellescape(b"%", true, true) }, b"'\\%'".to_vec());
        assert_eq!(unsafe { vim_strsave_shellescape(b"<cword>", true, true) }, b"'\\<cword>'".to_vec());
    }

    #[test]
    fn shellescape_do_special_false_leaves_special_vars_unescaped() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShellVarsGuard::plain_single_quote();
        assert_eq!(unsafe { vim_strsave_shellescape(b"%", false, false) }, b"'%'".to_vec());
    }

    #[test]
    fn shellescape_csh_like_shell_escapes_bang_even_without_do_special() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShellVarsGuard::set_shell(b"/bin/tcsh");
        // csh_like on its own (do_special=false) escapes '!' once.
        assert_eq!(unsafe { vim_strsave_shellescape(b"abc!", false, false) }, b"'abc\\!'".to_vec());
    }

    #[test]
    fn shellescape_csh_like_shell_with_do_special_escapes_bang_twice() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShellVarsGuard::set_shell(b"/bin/csh");
        // csh_like AND do_special both true: double backslash.
        assert_eq!(unsafe { vim_strsave_shellescape(b"abc!", true, true) }, b"'abc\\\\!'".to_vec());
    }

    #[test]
    fn shellescape_fish_like_shell_escapes_embedded_backslash() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShellVarsGuard::set_shell(b"/usr/bin/fish");
        assert_eq!(unsafe { vim_strsave_shellescape(b"a\\b", false, false) }, b"'a\\\\b'".to_vec());
    }

    #[test]
    fn shellescape_preserves_multibyte_characters() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShellVarsGuard::plain_single_quote();
        assert_eq!(
            unsafe { vim_strsave_shellescape("一二".as_bytes(), false, false) },
            "'一二'".as_bytes().to_vec()
        );
    }

    #[cfg(windows)]
    #[test]
    fn shellescape_uses_double_quotes_on_windows_without_shellslash() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShellVarsGuard::windows_double_quote();
        assert_eq!(unsafe { vim_strsave_shellescape(b"hello", false, false) }, b"\"hello\"".to_vec());
    }

    #[cfg(windows)]
    #[test]
    fn shellescape_escapes_embedded_double_quote_on_windows() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShellVarsGuard::windows_double_quote();
        assert_eq!(unsafe { vim_strsave_shellescape(b"say \"hi\"", false, false) }, b"\"say \"\"hi\"\"\"".to_vec());
    }
}
