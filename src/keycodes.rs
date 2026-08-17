//! Translated from `src/nvim/keycodes.c` (tractable core only).
//!
//! `keycodes.c` (~830 lines) is neovim's special-key name/termcap-code
//! lookup and conversion file - many of its remaining functions need
//! `mouse.c`'s mouse-event table (not translated) or
//! `replace_termcodes`/`trans_special`'s own substantial parsing
//! logic.
//!
//! Translated: [`name_to_mod_mask`], [`handle_x_keys`],
//! [`simplify_key`] - all pure, self-contained lookups needing only
//! [`crate::keycodes_defs`]'s own constant/table translations - and
//! [`vim_unescape_ks`], a self-contained in-place unescape needing
//! only [`crate::keycodes_defs::K_SPECIAL`]/
//! [`crate::keycodes_defs::KS_SPECIAL`]/[`crate::keycodes_defs::KE_FILLER`].
//!
//! Also translated: [`KEY_NAMES_TABLE`] (`key_names_table`) - the
//! original's own key-name-to-code lookup table, mechanically
//! transcribed from a pre-built copy of `keycode_names.generated.h`
//! (187 entries) via a throwaway Python extraction script, with every
//! entry's `key`/`is_alt`/`name` fields cross-checked (length
//! assertions on every transcribed name, a second pass verifying no
//! unresolved bare identifier slipped into the generated Rust source)
//! before trusting it - plus [`find_special_key_in_table`] and
//! [`get_special_key_code`], the table's own 2 real consumer
//! functions with no OTHER dependency.
//!
//! [`get_special_key_code`] deliberately does NOT replicate the
//! original's own `get_special_key_code_hash` - a machine-generated
//! PERFECT HASH function (via `src/gen/gen_keycodes.lua`'s
//! `hashy.hashy_hash`, the same code-generation family that produces
//! `option.c`'s own `find_option`-adjacent perfect hash) - since that
//! hash is purely a performance optimization over the SAME table, with
//! no behavioral difference from a plain, case-insensitive linear
//! scan (verified directly: `gen_keycodes.lua` passes `icase = true`
//! to `hashy_hash`, confirming the real lookup IS case-insensitive,
//! e.g. `"f1"` and `"F1"` both resolve to `K_F1`) - matching this
//! crate's established "translate the observable behavior, not the
//! exact optimization mechanism" precedent (e.g. `winrestcmd`'s own
//! single-pass `Vec` replacing the original's 2-pass C string
//! building).
//!
//! Also translated: [`extract_modifiers`] - folds Ctrl/Shift modifiers
//! into a single-byte key where possible (e.g. `"Shift-a"` -> `'A'`,
//! `"Ctrl-@"` -> [`crate::keycodes_defs::K_ZERO`]), needing only
//! already-real [`crate::macros_defs::ascii_isalpha`]/
//! [`crate::macros_defs::toupper_asc`]/[`crate::ascii_defs::ctrl_chr`].
//!
//! Also translated: [`add_char2buf`] (escapes `K_SPECIAL` while
//! copying a single character, needing only already-real
//! [`crate::mbyte::utf_char2bytes`]) and, now that it exists,
//! [`vim_strsave_escape_ks`] (the escaping counterpart of
//! [`vim_unescape_ks`], needing [`crate::mbyte::utf_ptr2char`]/
//! [`crate::mbyte::utf_ptr2len`], both already real). Neither has a
//! real translated caller yet (`api/vim.c`'s `nvim_replace_termcodes`,
//! `eval/funcs.c`'s `keytrans()` - which ALSO needs
//! `find_special_key`, still deferred below - `mapping.c`, and
//! `register.c`'s typeahead-buffer insertion are all real callers in
//! the original, none translated) - translated ahead of one anyway,
//! matching this crate's established "small, simple, mechanically
//! correct piece ahead of its real caller" precedent.
//!
//! Deferred: everything else - `get_special_key`/`get_special_key_name`/
//! `find_special_key`/`replace_termcodes`/`trans_special`.

#[derive(Clone, Copy)]
struct MouseTableEntry {
    pseudo_code: u8,
    button: i32,
    is_click: bool,
    is_drag: bool,
}

/// Encode a key and modifiers into Neovim's internal byte sequence
/// (`special_to_buf`).
#[must_use]
pub fn special_to_buf(key: i32, modifiers: i32, escape_ks: bool) -> Vec<u8> {
    let mut result = Vec::with_capacity(8);
    if modifiers != 0 {
        result.extend_from_slice(&[
            crate::keycodes_defs::K_SPECIAL,
            crate::keycodes_defs::KS_MODIFIER,
            modifiers as u8,
        ]);
    }

    if crate::keycodes_defs::is_special(key) {
        result.extend_from_slice(&[
            crate::keycodes_defs::K_SPECIAL,
            crate::keycodes_defs::key2termcap0(key),
            crate::keycodes_defs::key2termcap1(key),
        ]);
    } else if escape_ks {
        let mut encoded = [0; crate::mbyte_defs::MB_MAXBYTES * 3];
        let len = add_char2buf(key, &mut encoded);
        result.extend_from_slice(&encoded[..len]);
    } else {
        let mut encoded = [0; crate::mbyte_defs::MB_MAXCHAR];
        let len = crate::mbyte::utf_char2bytes(key, &mut encoded) as usize;
        result.extend_from_slice(&encoded[..len]);
    }
    result
}

/// Parse one `<...>` special-key name (`find_special_key`).
///
/// Returns `(key, modifiers, bytes_consumed, did_simplify)`, or
/// `None` when the opening text is not a valid special key.
#[must_use]
pub fn find_special_key(src: &[u8], flags: i32) -> Option<(i32, i32, usize, bool)> {
    if src.first() != Some(&b'<') {
        return None;
    }

    let end = src.len().checked_sub(1)?;
    let in_string = flags & crate::keycodes_defs::fsk::IN_STRING != 0;
    let start = usize::from(src.get(1) == Some(&b'*'));
    let effective_start = start;
    let mut last_dash = effective_start;
    let mut bp = effective_start + 1;

    while bp <= end
        && (src[bp] == b'-' || crate::ascii_defs::ascii_isident(i32::from(src[bp])))
    {
        if src[bp] == b'-' {
            last_dash = bp;
            if bp < end {
                let remaining = end - bp;
                let len = usize::try_from(unsafe {
                    crate::mbyte::utfc_ptr2len_len(&src[bp + 1..], remaining)
                })
                .unwrap_or(1)
                .max(1);
                if end - bp > len
                    && !(in_string && src[bp + 1] == b'"')
                    && src.get(bp + len + 1) == Some(&b'>')
                {
                    bp += len;
                } else if end - bp > 2
                    && in_string
                    && src.get(bp + 1) == Some(&b'\\')
                    && src.get(bp + 2) == Some(&b'"')
                    && src.get(bp + 3) == Some(&b'>')
                {
                    bp += 2;
                }
            }
        }

        if end.saturating_sub(bp) > 3
            && src.get(bp) == Some(&b't')
            && src.get(bp + 1) == Some(&b'_')
        {
            bp += 3;
        } else if end.saturating_sub(bp) > 4
            && src[bp..bp + 5].eq_ignore_ascii_case(b"char-")
        {
            let mut consumed = 0;
            crate::charset::vim_str2nr(
                &src[bp + 5..],
                None,
                Some(&mut consumed),
                crate::charset::STR2NR_ALL,
                None,
                None,
                0,
                true,
                None,
            );
            if consumed == 0 {
                return None;
            }
            bp += usize::try_from(consumed).ok()? + 5;
            break;
        }
        bp += 1;
    }

    if bp > end || src[bp] != b'>' {
        return None;
    }
    let consumed = bp + 1;

    let mut modifiers = 0;
    let mut modp = effective_start + 1;
    while modp < last_dash {
        if src[modp] != b'-' {
            let bit = name_to_mod_mask(i32::from(src[modp]));
            if bit == 0 {
                return None;
            }
            modifiers |= bit;
        }
        modp += 1;
    }

    let name_start = last_dash + 1;
    let mut key = if src
        .get(name_start..name_start + 5)
        .is_some_and(|name| name.eq_ignore_ascii_case(b"char-"))
        && src.get(name_start + 5).is_some_and(u8::is_ascii_digit)
    {
        let mut len = 0;
        let mut value = 0;
        crate::charset::vim_str2nr(
            &src[name_start + 5..],
            None,
            Some(&mut len),
            crate::charset::STR2NR_ALL,
            None,
            Some(&mut value),
            0,
            true,
            None,
        );
        if len == 0 {
            return None;
        }
        value as i32
    } else {
        let (off, len) = if in_string
            && src.get(name_start) == Some(&b'\\')
            && src.get(name_start + 1) == Some(&b'"')
        {
            (1, 2)
        } else {
            (
                0,
                usize::try_from(unsafe { crate::mbyte::utfc_ptr2len(&src[name_start..]) })
                    .unwrap_or(1)
                    .max(1),
            )
        };
        if modifiers != 0 && src.get(last_dash + len + 1) == Some(&b'>') {
            crate::mbyte::utf_ptr2char(&src[name_start + off..])
        } else {
            let mut key = get_special_key_code(&src[name_start + off..bp]);
            if flags & crate::keycodes_defs::fsk::KEEP_X_KEY == 0 {
                key = handle_x_keys(key);
            }
            key
        }
    };

    if key == 0 {
        return None;
    }
    key = simplify_key(key, &mut modifiers);
    if flags & crate::keycodes_defs::fsk::KEYCODE == 0 {
        key = match key {
            crate::keycodes_defs::K_BS => i32::from(crate::ascii_defs::BS),
            crate::keycodes_defs::K_DEL | crate::keycodes_defs::K_KDEL => {
                i32::from(crate::ascii_defs::DEL)
            }
            _ => key,
        };
    }

    let mut did_simplify = false;
    if !crate::keycodes_defs::is_special(key) {
        key = extract_modifiers(
            key,
            &mut modifiers,
            flags & crate::keycodes_defs::fsk::SIMPLIFY != 0,
            Some(&mut did_simplify),
        );
    }
    Some((key, modifiers, consumed, did_simplify))
}

/// Return the printable `<...>` name for a key and modifiers
/// (`get_special_key_name`).
///
/// # Safety
/// May consult the current character table through
/// `charset::vim_isprintc`/`transchar`.
#[must_use]
pub unsafe fn get_special_key_name(mut key: i32, mut modifiers: i32) -> Vec<u8> {
    let mut result = vec![b'<'];

    if crate::keycodes_defs::is_special(key)
        && crate::keycodes_defs::key2termcap0(key) == crate::keycodes_defs::KS_KEY
    {
        key = i32::from(crate::keycodes_defs::key2termcap1(key));
    }

    if crate::keycodes_defs::is_special(key) {
        for &(mask, key0, key1, base0, base1) in crate::keycodes_defs::MODIFIER_KEYS_TABLE {
            if crate::keycodes_defs::key2termcap0(key) == key0
                && crate::keycodes_defs::key2termcap1(key) == key1
            {
                modifiers |= i32::from(mask);
                key = crate::keycodes_defs::termcap2key(base0, base1);
                break;
            }
        }
    }

    let table_idx = find_special_key_in_table(key);
    if key > 0
        && crate::mbyte::utf_char2len(key) == 1
        && table_idx < 0
        && !unsafe { crate::charset::vim_isprintc(key) }
        && key < i32::from(b' ')
    {
        key += i32::from(b'@');
        modifiers |= i32::from(crate::keycodes_defs::MOD_MASK_CTRL);
    }

    for entry in crate::keycodes_defs::MOD_MASK_TABLE
        .iter()
        .take_while(|entry| entry.name != b'A')
    {
        if modifiers & i32::from(entry.mod_mask) == i32::from(entry.mod_flag) {
            result.extend_from_slice(&[entry.name, b'-']);
        }
    }

    if table_idx < 0 {
        if crate::keycodes_defs::is_special(key) {
            result.extend_from_slice(&[
                b't',
                b'_',
                crate::keycodes_defs::key2termcap0(key),
                crate::keycodes_defs::key2termcap1(key),
            ]);
        } else {
            let len = crate::mbyte::utf_char2len(key);
            if len == 1 && unsafe { crate::charset::vim_isprintc(key) } {
                result.push(key as u8);
            } else if len > 1 {
                let mut encoded = [0; crate::mbyte_defs::MB_MAXCHAR];
                let written = crate::mbyte::utf_char2bytes(key, &mut encoded) as usize;
                result.extend_from_slice(&encoded[..written]);
            } else {
                result.extend_from_slice(&unsafe { crate::charset::transchar(key) });
            }
        }
    } else {
        result.extend_from_slice(KEY_NAMES_TABLE[table_idx as usize].name.as_bytes());
    }
    result.push(b'>');
    result
}

macro_rules! mouse_entry {
    ($code:ident, $button:ident, $click:literal, $drag:literal) => {
        MouseTableEntry {
            pseudo_code: crate::keycodes_defs::$code,
            button: crate::mouse::$button,
            is_click: $click,
            is_drag: $drag,
        }
    };
}

const MOUSE_TABLE: [MouseTableEntry; 17] = [
    mouse_entry!(KE_LEFTMOUSE, MOUSE_LEFT, true, false),
    mouse_entry!(KE_LEFTDRAG, MOUSE_LEFT, false, true),
    mouse_entry!(KE_LEFTRELEASE, MOUSE_LEFT, false, false),
    mouse_entry!(KE_MIDDLEMOUSE, MOUSE_MIDDLE, true, false),
    mouse_entry!(KE_MIDDLEDRAG, MOUSE_MIDDLE, false, true),
    mouse_entry!(KE_MIDDLERELEASE, MOUSE_MIDDLE, false, false),
    mouse_entry!(KE_RIGHTMOUSE, MOUSE_RIGHT, true, false),
    mouse_entry!(KE_RIGHTDRAG, MOUSE_RIGHT, false, true),
    mouse_entry!(KE_RIGHTRELEASE, MOUSE_RIGHT, false, false),
    mouse_entry!(KE_X1MOUSE, MOUSE_X1, true, false),
    mouse_entry!(KE_X1DRAG, MOUSE_X1, false, true),
    mouse_entry!(KE_X1RELEASE, MOUSE_X1, false, false),
    mouse_entry!(KE_X2MOUSE, MOUSE_X2, true, false),
    mouse_entry!(KE_X2DRAG, MOUSE_X2, false, true),
    mouse_entry!(KE_X2RELEASE, MOUSE_X2, false, false),
    mouse_entry!(KE_MOUSEMOVE, MOUSE_RELEASE, false, true),
    mouse_entry!(KE_IGNORE, MOUSE_RELEASE, false, false),
];

/// Decode a pseudo mouse keycode (`get_mouse_button`).
///
/// The original writes click/drag booleans through out-parameters;
/// they are returned with the button as a tuple here.
#[must_use]
pub fn get_mouse_button(code: i32) -> (i32, bool, bool) {
    MOUSE_TABLE
        .iter()
        .find(|entry| i32::from(entry.pseudo_code) == code)
        .map_or((0, false, false), |entry| {
            (entry.button, entry.is_click, entry.is_drag)
        })
}

use crate::ascii_defs::TAB;
use crate::keycodes_defs::{key2termcap0, key2termcap1, termcap2key, MODIFIER_KEYS_TABLE, 
    MOD_MASK_CTRL, MOD_MASK_SHIFT, MOD_MASK_TABLE, K_BS, K_COMMAND, K_DEL, K_DOWN, K_DROP, 
    K_END, K_F1, K_F10, K_F11, K_F12, K_F13, K_F14, K_F15, K_F16, K_F17, K_F18, K_F19, K_F2, 
    K_F20, K_F21, K_F22, K_F23, K_F24, K_F25, K_F26, K_F27, K_F28, K_F29, K_F3, K_F30, K_F31, 
    K_F32, K_F33, K_F34, K_F35, K_F36, K_F37, K_F38, K_F39, K_F4, K_F40, K_F41, K_F42, K_F43, 
    K_F44, K_F45, K_F46, K_F47, K_F48, K_F49, K_F5, K_F50, K_F51, K_F52, K_F53, K_F54, K_F55, 
    K_F56, K_F57, K_F58, K_F59, K_F6, K_F60, K_F61, K_F62, K_F63, K_F7, K_F8, K_F9, K_FIND, 
    K_HELP, K_HOME, K_IGNORE, K_INS, K_K0, K_K1, K_K2, K_K3, K_K4, K_K5, K_K6, K_K7, K_K8, 
    K_K9, K_KCOMMA, K_KDEL, K_KDIVIDE, K_KDOWN, K_KEND, K_KENTER, K_KEQUAL, K_KHOME, K_KINS, 
    K_KLEFT, K_KMINUS, K_KMULTIPLY, K_KORIGIN, K_KPAGEDOWN, K_KPAGEUP, K_KPLUS, K_KPOINT, 
    K_KRIGHT, K_KSELECT, K_KUP, K_LEFT, K_LEFTDRAG, K_LEFTMOUSE, K_LEFTMOUSE_NM, K_LEFTRELEASE, 
    K_LEFTRELEASE_NM, K_MIDDLEDRAG, K_MIDDLEMOUSE, K_MIDDLERELEASE, K_MOUSE, K_MOUSEDOWN, 
    K_MOUSELEFT, K_MOUSEMOVE, K_MOUSERIGHT, K_MOUSEUP, K_PAGEDOWN, K_PAGEUP, K_PLUG, K_RIGHT, 
    K_RIGHTDRAG, K_RIGHTMOUSE, K_RIGHTRELEASE, K_SNR, K_S_F1, K_S_F2, K_S_F3, K_S_F4, K_S_TAB, 
    K_S_XF1, K_S_XF2, K_S_XF3, K_S_XF4, K_TAB, K_UNDO, K_UP, K_X1DRAG, K_X1MOUSE, K_X1RELEASE, 
    K_X2DRAG, K_X2MOUSE, K_X2RELEASE, K_XDOWN, K_XEND, K_XF1, K_XF2, K_XF3, K_XF4, K_XHOME, 
    K_XLEFT, K_XRIGHT, K_XUP, K_ZEND, K_ZERO, K_ZHOME};

/// Returns the [`crate::keycodes_defs::MOD_MASK_TABLE`] modifier-mask
/// bit corresponding to modifier letter `c` (e.g. `'S'` for shift, `'C'`
/// for ctrl), or `0` if `c` isn't a recognized modifier letter
/// (`name_to_mod_mask`). `c` is matched case-insensitively (uppercased
/// via [`crate::macros_defs::toupper_asc`] first, matching the
/// original).
#[must_use]
pub fn name_to_mod_mask(c: i32) -> i32 {
    let c = crate::macros_defs::toupper_asc(c);
    for entry in MOD_MASK_TABLE {
        if c == i32::from(entry.name) {
            return i32::from(entry.mod_flag);
        }
    }
    0
}

/// Changes an `<xKey>`-style key code (e.g. `<xUp>`, `<xF1>`) to its
/// plain equivalent (e.g. `<Up>`, `<F1>`) - `key` is returned unchanged
/// if it isn't one of the recognized `<x...>`/`<z...>` codes
/// (`handle_x_keys`).
#[must_use]
pub fn handle_x_keys(key: i32) -> i32 {
    match key {
        K_XUP => K_UP,
        K_XDOWN => K_DOWN,
        K_XLEFT => K_LEFT,
        K_XRIGHT => K_RIGHT,
        K_XHOME | K_ZHOME => K_HOME,
        K_XEND | K_ZEND => K_END,
        K_XF1 => K_F1,
        K_XF2 => K_F2,
        K_XF3 => K_F3,
        K_XF4 => K_F4,
        K_S_XF1 => K_S_F1,
        K_S_XF2 => K_S_F2,
        K_S_XF3 => K_S_F3,
        K_S_XF4 => K_S_F4,
        _ => key,
    }
}

/// Simplifies `key` + `*modifiers` into a single combined key code when
/// there's a dedicated termcap code for that specific
/// key-plus-one-modifier combination, clearing the now-redundant
/// modifier bit from `*modifiers` (`simplify_key`). Returns `key`
/// unchanged (and leaves `*modifiers` untouched) if no such combination
/// applies.
///
/// TAB + Shift is a special case with its own dedicated check (matching
/// the original) since `TAB`'s own plain ASCII value isn't itself a
/// `termcap2key`-encoded "special key", unlike every other entry in
/// [`MODIFIER_KEYS_TABLE`].
pub fn simplify_key(key: i32, modifiers: &mut i32) -> i32 {
    if *modifiers & i32::from(MOD_MASK_SHIFT | MOD_MASK_CTRL) == 0 {
        return key;
    }

    // TAB is a special case.
    if key == i32::from(TAB) && *modifiers & i32::from(MOD_MASK_SHIFT) != 0 {
        *modifiers &= !i32::from(MOD_MASK_SHIFT);
        return K_S_TAB;
    }

    let key0 = key2termcap0(key);
    let key1 = key2termcap1(key);
    for &(mod_mask, with0, with1, without0, without1) in MODIFIER_KEYS_TABLE {
        if key0 == without0 && key1 == without1 && *modifiers & i32::from(mod_mask) != 0 {
            *modifiers &= !i32::from(mod_mask);
            return termcap2key(with0, with1);
        }
    }
    key
}

/// Remove escaping from `K_SPECIAL` characters - the reverse of
/// [`vim_strsave_escape_ks`]. Works in place, returning the number of
/// bytes in the unescaped result (`vim_unescape_ks`).
///
/// Modeled as `p: &mut [u8]` (in place, like the original's own
/// `char *p` in/out buffer) rather than returning a fresh `Vec<u8>`:
/// every real caller (`mapping.c`, `lua/executor.c`, `register.c`)
/// already owns a mutable buffer it wants shrunk in place, matching
/// this crate's established convention for genuinely in-place C
/// buffer algorithms (e.g. `charset::rl_mirror_ascii`).
#[must_use]
pub fn vim_unescape_ks(p: &mut [u8]) -> usize {
    let mut s = 0usize;
    let mut d = 0usize;
    while s < p.len() && p[s] != 0 {
        if p[s] == crate::keycodes_defs::K_SPECIAL
            && p.get(s + 1).copied() == Some(crate::keycodes_defs::KS_SPECIAL)
            && p.get(s + 2).copied() == Some(crate::keycodes_defs::KE_FILLER)
        {
            p[d] = crate::keycodes_defs::K_SPECIAL;
            d += 1;
            s += 3;
        } else {
            p[d] = p[s];
            d += 1;
            s += 1;
        }
    }
    d
}

/// Add character `c` to buffer `s`, escaping the special meaning of
/// `K_SPECIAL` and handling multi-byte characters (`add_char2buf`).
///
/// Writes starting at `s[0]` and returns the number of bytes written -
/// replacing the original's own "return a pointer to just after the
/// added bytes" convention (the caller advances its own write position
/// by that count instead, matching this crate's established
/// bytes-written idiom for buffer-cursor C functions). The caller's
/// own buffer must have room for at least `MB_MAXBYTES + 1` bytes from
/// `s[0]`, matching the original's own documented contract (up to 6
/// UTF-8 bytes, each possibly expanding to 3 bytes if it equals
/// `K_SPECIAL`, comfortably fits within `MB_MAXBYTES + 1 = 22`).
///
/// # Panics
/// If `s` has fewer than 3 times [`crate::mbyte::utf_char2len`]`(c)`
/// bytes of room (matching [`crate::mbyte::utf_char2bytes`]'s own
/// panic contract, propagated here).
#[must_use]
pub fn add_char2buf(c: i32, s: &mut [u8]) -> usize {
    let mut temp = [0u8; crate::mbyte_defs::MB_MAXBYTES + 1];
    let len = crate::mbyte::utf_char2bytes(c, &mut temp) as usize;
    let mut written = 0;
    for &byte in &temp[..len] {
        // Need to escape K_SPECIAL like in the typeahead buffer.
        if byte == crate::keycodes_defs::K_SPECIAL {
            s[written] = crate::keycodes_defs::K_SPECIAL;
            s[written + 1] = crate::keycodes_defs::KS_SPECIAL;
            s[written + 2] = crate::keycodes_defs::KE_FILLER;
            written += 3;
        } else {
            s[written] = byte;
            written += 1;
        }
    }
    written
}

/// Copy `p` to a freshly-allocated buffer, escaping `K_SPECIAL` so the
/// result can be put in the typeahead buffer (`vim_strsave_escape_ks`).
///
/// `p` is scanned up to (not including) its first NUL byte or the end
/// of the slice, whichever comes first - matching the original's own
/// NUL-terminated-`char *` convention. The returned `Vec<u8>` carries
/// NO trailing NUL of its own, matching this crate's established
/// "`.len()` is authoritative" convention for a freshly-produced byte
/// buffer that isn't line/memline-shaped storage.
#[must_use]
pub fn vim_strsave_escape_ks(p: &[u8]) -> Vec<u8> {
    let p = match p.iter().position(|&b| b == 0) {
        Some(nul_at) => &p[..nul_at],
        None => p,
    };
    // Need a buffer to hold up to three times as much (K_SPECIAL
    // escaping). Four in case of an illegal UTF-8 byte: 0xc0 -> 0xc3
    // K_SPECIAL KS_SPECIAL KE_FILLER.
    let mut res = Vec::with_capacity(p.len() * 4);
    let mut s = 0usize;
    while s < p.len() {
        if p[s] == crate::keycodes_defs::K_SPECIAL && p.get(s + 1).is_some() && p.get(s + 2).is_some() {
            // Copy special key unmodified. p was already truncated at
            // its first NUL above, so "p[s+1]/p[s+2] exist" here is
            // exactly the original's own "s[1] != NUL && s[2] != NUL".
            res.push(p[s]);
            res.push(p[s + 1]);
            res.push(p[s + 2]);
            s += 3;
        } else {
            // Add character, possibly multi-byte, to destination,
            // escaping K_SPECIAL. Be careful, it can be an illegal
            // byte!
            let c = crate::mbyte::utf_ptr2char(&p[s..]);
            let mut buf = [0u8; crate::mbyte_defs::MB_MAXBYTES + 1];
            let written = add_char2buf(c, &mut buf);
            res.extend_from_slice(&buf[..written]);
            s += crate::mbyte::utf_ptr2len(&p[s..]) as usize;
        }
    }
    res
}

/// One entry of `KEY_NAMES_TABLE` (`struct key_name_entry`).
pub struct KeyNameEntry {
    /// Special key code or ASCII value (`key`).
    pub key: i32,
    /// Is an alternative name (`is_alt`).
    pub is_alt: bool,
    /// Name of key (`name`).
    pub name: &'static str,
}

/// `key_names_table` - the special-key name/code lookup table
/// (mechanically transcribed - see this module's own doc comment for
/// the full methodology). Every entry is in the ORIGINAL's own
/// natural declaration order (NOT the hash-bucket order the real
/// generated header happens to store it in) - order doesn't matter
/// here, since both `find_special_key_in_table`/
/// `get_special_key_code` use a plain linear scan, not the
/// original's own perfect hash.
pub static KEY_NAMES_TABLE: &[KeyNameEntry] = &[
    KeyNameEntry { key: K_K0, is_alt: false, name: "k0" },
    KeyNameEntry { key: K_F1, is_alt: false, name: "F1" },
    KeyNameEntry { key: K_K1, is_alt: false, name: "k1" },
    KeyNameEntry { key: K_F2, is_alt: false, name: "F2" },
    KeyNameEntry { key: K_K2, is_alt: false, name: "k2" },
    KeyNameEntry { key: K_F3, is_alt: false, name: "F3" },
    KeyNameEntry { key: K_K3, is_alt: false, name: "k3" },
    KeyNameEntry { key: K_F4, is_alt: false, name: "F4" },
    KeyNameEntry { key: K_K4, is_alt: false, name: "k4" },
    KeyNameEntry { key: K_F5, is_alt: false, name: "F5" },
    KeyNameEntry { key: K_K5, is_alt: false, name: "k5" },
    KeyNameEntry { key: K_F6, is_alt: false, name: "F6" },
    KeyNameEntry { key: K_K6, is_alt: false, name: "k6" },
    KeyNameEntry { key: K_F7, is_alt: false, name: "F7" },
    KeyNameEntry { key: K_K7, is_alt: false, name: "k7" },
    KeyNameEntry { key: K_F8, is_alt: false, name: "F8" },
    KeyNameEntry { key: K_K8, is_alt: false, name: "k8" },
    KeyNameEntry { key: K_F9, is_alt: false, name: "F9" },
    KeyNameEntry { key: K_K9, is_alt: false, name: "k9" },
    KeyNameEntry { key: (crate::ascii_defs::NL as i32), is_alt: true, name: "LF" },
    KeyNameEntry { key: (crate::ascii_defs::NL as i32), is_alt: false, name: "NL" },
    KeyNameEntry { key: K_UP, is_alt: false, name: "Up" },
    KeyNameEntry { key: (crate::ascii_defs::CAR as i32), is_alt: false, name: "CR" },
    KeyNameEntry { key: K_BS, is_alt: false, name: "BS" },
    KeyNameEntry { key: (b'<' as i32), is_alt: false, name: "lt" },
    KeyNameEntry { key: K_F10, is_alt: false, name: "F10" },
    KeyNameEntry { key: K_F20, is_alt: false, name: "F20" },
    KeyNameEntry { key: K_F30, is_alt: false, name: "F30" },
    KeyNameEntry { key: K_F40, is_alt: false, name: "F40" },
    KeyNameEntry { key: K_F50, is_alt: false, name: "F50" },
    KeyNameEntry { key: K_F60, is_alt: false, name: "F60" },
    KeyNameEntry { key: K_KINS, is_alt: true, name: "KP0" },
    KeyNameEntry { key: K_F11, is_alt: false, name: "F11" },
    KeyNameEntry { key: K_F21, is_alt: false, name: "F21" },
    KeyNameEntry { key: K_F31, is_alt: false, name: "F31" },
    KeyNameEntry { key: K_F41, is_alt: false, name: "F41" },
    KeyNameEntry { key: K_F51, is_alt: false, name: "F51" },
    KeyNameEntry { key: K_F61, is_alt: false, name: "F61" },
    KeyNameEntry { key: K_KEND, is_alt: true, name: "KP1" },
    KeyNameEntry { key: K_XF1, is_alt: false, name: "xF1" },
    KeyNameEntry { key: K_F12, is_alt: false, name: "F12" },
    KeyNameEntry { key: K_F22, is_alt: false, name: "F22" },
    KeyNameEntry { key: K_F32, is_alt: false, name: "F32" },
    KeyNameEntry { key: K_F42, is_alt: false, name: "F42" },
    KeyNameEntry { key: K_F52, is_alt: false, name: "F52" },
    KeyNameEntry { key: K_F62, is_alt: false, name: "F62" },
    KeyNameEntry { key: K_KDOWN, is_alt: true, name: "KP2" },
    KeyNameEntry { key: K_XF2, is_alt: false, name: "xF2" },
    KeyNameEntry { key: K_F13, is_alt: false, name: "F13" },
    KeyNameEntry { key: K_F23, is_alt: false, name: "F23" },
    KeyNameEntry { key: K_F33, is_alt: false, name: "F33" },
    KeyNameEntry { key: K_F43, is_alt: false, name: "F43" },
    KeyNameEntry { key: K_F53, is_alt: false, name: "F53" },
    KeyNameEntry { key: K_F63, is_alt: false, name: "F63" },
    KeyNameEntry { key: K_KPAGEDOWN, is_alt: true, name: "KP3" },
    KeyNameEntry { key: K_XF3, is_alt: false, name: "xF3" },
    KeyNameEntry { key: K_F14, is_alt: false, name: "F14" },
    KeyNameEntry { key: K_F24, is_alt: false, name: "F24" },
    KeyNameEntry { key: K_F34, is_alt: false, name: "F34" },
    KeyNameEntry { key: K_F44, is_alt: false, name: "F44" },
    KeyNameEntry { key: K_F54, is_alt: false, name: "F54" },
    KeyNameEntry { key: K_KLEFT, is_alt: true, name: "KP4" },
    KeyNameEntry { key: K_XF4, is_alt: false, name: "xF4" },
    KeyNameEntry { key: K_F15, is_alt: false, name: "F15" },
    KeyNameEntry { key: K_F25, is_alt: false, name: "F25" },
    KeyNameEntry { key: K_F35, is_alt: false, name: "F35" },
    KeyNameEntry { key: K_F45, is_alt: false, name: "F45" },
    KeyNameEntry { key: K_F55, is_alt: false, name: "F55" },
    KeyNameEntry { key: K_KORIGIN, is_alt: true, name: "KP5" },
    KeyNameEntry { key: K_F16, is_alt: false, name: "F16" },
    KeyNameEntry { key: K_F26, is_alt: false, name: "F26" },
    KeyNameEntry { key: K_F36, is_alt: false, name: "F36" },
    KeyNameEntry { key: K_F46, is_alt: false, name: "F46" },
    KeyNameEntry { key: K_F56, is_alt: false, name: "F56" },
    KeyNameEntry { key: K_KRIGHT, is_alt: true, name: "KP6" },
    KeyNameEntry { key: K_F17, is_alt: false, name: "F17" },
    KeyNameEntry { key: K_F27, is_alt: false, name: "F27" },
    KeyNameEntry { key: K_F37, is_alt: false, name: "F37" },
    KeyNameEntry { key: K_F47, is_alt: false, name: "F47" },
    KeyNameEntry { key: K_F57, is_alt: false, name: "F57" },
    KeyNameEntry { key: K_KHOME, is_alt: true, name: "KP7" },
    KeyNameEntry { key: K_F18, is_alt: false, name: "F18" },
    KeyNameEntry { key: K_F28, is_alt: false, name: "F28" },
    KeyNameEntry { key: K_F38, is_alt: false, name: "F38" },
    KeyNameEntry { key: K_F48, is_alt: false, name: "F48" },
    KeyNameEntry { key: K_F58, is_alt: false, name: "F58" },
    KeyNameEntry { key: K_KUP, is_alt: true, name: "KP8" },
    KeyNameEntry { key: K_F19, is_alt: false, name: "F19" },
    KeyNameEntry { key: K_F29, is_alt: false, name: "F29" },
    KeyNameEntry { key: K_F39, is_alt: false, name: "F39" },
    KeyNameEntry { key: K_F49, is_alt: false, name: "F49" },
    KeyNameEntry { key: K_F59, is_alt: false, name: "F59" },
    KeyNameEntry { key: K_KPAGEUP, is_alt: true, name: "KP9" },
    KeyNameEntry { key: (crate::ascii_defs::TAB as i32), is_alt: false, name: "Tab" },
    KeyNameEntry { key: K_TAB, is_alt: false, name: "Tab" },
    KeyNameEntry { key: (crate::ascii_defs::ESC as i32), is_alt: false, name: "Esc" },
    KeyNameEntry { key: K_COMMAND, is_alt: false, name: "Cmd" },
    KeyNameEntry { key: K_END, is_alt: false, name: "End" },
    KeyNameEntry { key: (crate::ascii_defs::CSI as i32), is_alt: false, name: "CSI" },
    KeyNameEntry { key: K_DEL, is_alt: false, name: "Del" },
    KeyNameEntry { key: K_ZERO, is_alt: false, name: "Nul" },
    KeyNameEntry { key: K_KUP, is_alt: false, name: "kUp" },
    KeyNameEntry { key: K_XUP, is_alt: false, name: "xUp" },
    KeyNameEntry { key: (b'|' as i32), is_alt: false, name: "Bar" },
    KeyNameEntry { key: K_SNR, is_alt: false, name: "SNR" },
    KeyNameEntry { key: K_INS, is_alt: true, name: "Ins" },
    KeyNameEntry { key: K_DOWN, is_alt: false, name: "Down" },
    KeyNameEntry { key: K_DROP, is_alt: false, name: "Drop" },
    KeyNameEntry { key: K_FIND, is_alt: false, name: "Find" },
    KeyNameEntry { key: K_HELP, is_alt: false, name: "Help" },
    KeyNameEntry { key: K_HOME, is_alt: false, name: "Home" },
    KeyNameEntry { key: K_KDEL, is_alt: false, name: "kDel" },
    KeyNameEntry { key: K_KEND, is_alt: false, name: "kEnd" },
    KeyNameEntry { key: K_LEFT, is_alt: false, name: "Left" },
    KeyNameEntry { key: K_PLUG, is_alt: false, name: "Plug" },
    KeyNameEntry { key: K_UNDO, is_alt: false, name: "Undo" },
    KeyNameEntry { key: K_XEND, is_alt: false, name: "xEnd" },
    KeyNameEntry { key: K_ZEND, is_alt: false, name: "zEnd" },
    KeyNameEntry { key: K_KDOWN, is_alt: false, name: "kDown" },
    KeyNameEntry { key: K_XDOWN, is_alt: false, name: "xDown" },
    KeyNameEntry { key: K_KHOME, is_alt: false, name: "kHome" },
    KeyNameEntry { key: K_XHOME, is_alt: false, name: "xHome" },
    KeyNameEntry { key: K_ZHOME, is_alt: false, name: "zHome" },
    KeyNameEntry { key: K_RIGHT, is_alt: false, name: "Right" },
    KeyNameEntry { key: K_KLEFT, is_alt: false, name: "kLeft" },
    KeyNameEntry { key: K_XLEFT, is_alt: false, name: "xLeft" },
    KeyNameEntry { key: (crate::ascii_defs::CAR as i32), is_alt: true, name: "Enter" },
    KeyNameEntry { key: K_MOUSE, is_alt: false, name: "Mouse" },
    KeyNameEntry { key: K_KDIVIDE, is_alt: true, name: "KPDiv" },
    KeyNameEntry { key: K_KPLUS, is_alt: false, name: "kPlus" },
    KeyNameEntry { key: (b' ' as i32), is_alt: false, name: "Space" },
    KeyNameEntry { key: (crate::ascii_defs::ESC as i32), is_alt: true, name: "Escape" },
    KeyNameEntry { key: K_X1DRAG, is_alt: false, name: "X1Drag" },
    KeyNameEntry { key: K_X2DRAG, is_alt: false, name: "X2Drag" },
    KeyNameEntry { key: K_PAGEUP, is_alt: false, name: "PageUp" },
    KeyNameEntry { key: K_KMINUS, is_alt: false, name: "kMinus" },
    KeyNameEntry { key: K_KRIGHT, is_alt: false, name: "kRight" },
    KeyNameEntry { key: K_XRIGHT, is_alt: false, name: "xRight" },
    KeyNameEntry { key: (b'\\' as i32), is_alt: false, name: "Bslash" },
    KeyNameEntry { key: K_DEL, is_alt: true, name: "Delete" },
    KeyNameEntry { key: K_KSELECT, is_alt: false, name: "Select" },
    KeyNameEntry { key: K_KMULTIPLY, is_alt: true, name: "KPMult" },
    KeyNameEntry { key: K_IGNORE, is_alt: false, name: "Ignore" },
    KeyNameEntry { key: K_KENTER, is_alt: false, name: "kEnter" },
    KeyNameEntry { key: K_KCOMMA, is_alt: false, name: "kComma" },
    KeyNameEntry { key: K_KPOINT, is_alt: false, name: "kPoint" },
    KeyNameEntry { key: K_KPLUS, is_alt: true, name: "KPPlus" },
    KeyNameEntry { key: K_KEQUAL, is_alt: false, name: "kEqual" },
    KeyNameEntry { key: K_INS, is_alt: false, name: "Insert" },
    KeyNameEntry { key: (crate::ascii_defs::CAR as i32), is_alt: true, name: "Return" },
    KeyNameEntry { key: K_KPAGEUP, is_alt: false, name: "kPageUp" },
    KeyNameEntry { key: K_KCOMMA, is_alt: true, name: "KPComma" },
    KeyNameEntry { key: K_KENTER, is_alt: true, name: "KPEnter" },
    KeyNameEntry { key: K_KDIVIDE, is_alt: false, name: "kDivide" },
    KeyNameEntry { key: K_KMINUS, is_alt: true, name: "KPMinus" },
    KeyNameEntry { key: K_X1MOUSE, is_alt: false, name: "X1Mouse" },
    KeyNameEntry { key: K_X2MOUSE, is_alt: false, name: "X2Mouse" },
    KeyNameEntry { key: K_KINS, is_alt: false, name: "kInsert" },
    KeyNameEntry { key: K_KORIGIN, is_alt: false, name: "kOrigin" },
    KeyNameEntry { key: K_MOUSEUP, is_alt: true, name: "MouseUp" },
    KeyNameEntry { key: (crate::ascii_defs::NL as i32), is_alt: true, name: "NewLine" },
    KeyNameEntry { key: K_KEQUAL, is_alt: true, name: "KPEquals" },
    KeyNameEntry { key: K_LEFTDRAG, is_alt: false, name: "LeftDrag" },
    KeyNameEntry { key: K_PAGEDOWN, is_alt: false, name: "PageDown" },
    KeyNameEntry { key: (crate::ascii_defs::NL as i32), is_alt: true, name: "LineFeed" },
    KeyNameEntry { key: K_KDEL, is_alt: true, name: "KPPeriod" },
    KeyNameEntry { key: K_BS, is_alt: true, name: "BackSpace" },
    KeyNameEntry { key: K_KMULTIPLY, is_alt: false, name: "kMultiply" },
    KeyNameEntry { key: K_KPAGEDOWN, is_alt: false, name: "kPageDown" },
    KeyNameEntry { key: K_LEFTMOUSE, is_alt: false, name: "LeftMouse" },
    KeyNameEntry { key: K_MOUSEDOWN, is_alt: true, name: "MouseDown" },
    KeyNameEntry { key: K_MOUSEMOVE, is_alt: false, name: "MouseMove" },
    KeyNameEntry { key: K_RIGHTDRAG, is_alt: false, name: "RightDrag" },
    KeyNameEntry { key: K_X1RELEASE, is_alt: false, name: "X1Release" },
    KeyNameEntry { key: K_X2RELEASE, is_alt: false, name: "X2Release" },
    KeyNameEntry { key: K_MIDDLEDRAG, is_alt: false, name: "MiddleDrag" },
    KeyNameEntry { key: K_RIGHTMOUSE, is_alt: false, name: "RightMouse" },
    KeyNameEntry { key: K_MIDDLEMOUSE, is_alt: false, name: "MiddleMouse" },
    KeyNameEntry { key: K_LEFTMOUSE_NM, is_alt: false, name: "LeftMouseNM" },
    KeyNameEntry { key: K_LEFTRELEASE, is_alt: false, name: "LeftRelease" },
    KeyNameEntry { key: K_RIGHTRELEASE, is_alt: false, name: "RightRelease" },
    KeyNameEntry { key: K_LEFTRELEASE_NM, is_alt: false, name: "LeftReleaseNM" },
    KeyNameEntry { key: K_MIDDLERELEASE, is_alt: false, name: "MiddleRelease" },
    KeyNameEntry { key: K_MOUSEDOWN, is_alt: false, name: "ScrollWheelUp" },
    KeyNameEntry { key: K_MOUSEUP, is_alt: false, name: "ScrollWheelDown" },
    KeyNameEntry { key: K_MOUSERIGHT, is_alt: false, name: "ScrollWheelLeft" },
    KeyNameEntry { key: K_MOUSELEFT, is_alt: false, name: "ScrollWheelRight" },
];

/// Find a table index for special key `c` in `KEY_NAMES_TABLE`,
/// skipping alternate-name (`is_alt`) entries - used to look up a
/// key's own CANONICAL display name (`find_special_key_in_table`).
/// Returns `-1` if `c` isn't a recognized special key.
#[must_use]
pub fn find_special_key_in_table(c: i32) -> i32 {
    for (i, entry) in KEY_NAMES_TABLE.iter().enumerate() {
        if c == entry.key && !entry.is_alt {
            return i as i32;
        }
    }
    -1
}

/// Find the special key with the given name (`get_special_key_code`).
///
/// `name` does not have to end with a NUL byte - matching stops
/// before the first non-identifier byte (` . `/` _ `/
/// alphanumeric, via [`crate::ascii_defs::ascii_isident`]). If
/// `name` starts with `"t_"` the next two bytes are interpreted as
/// a termcap name directly, bypassing the table entirely.
///
/// The real lookup against [`KEY_NAMES_TABLE`] is
/// **case-insensitive** (matching the original's own generated perfect
/// hash, built with `icase = true` - see this module's own doc
/// comment) - translated here as a plain linear scan comparing
/// lowercased bytes, rather than replicating the hash itself.
///
/// Returns the key code, or `0` if not found.
#[must_use]
pub fn get_special_key_code(name: &[u8]) -> i32 {
    if name.first().copied() == Some(b't')
        && name.get(1).copied() == Some(b'_')
        && name.get(2).is_some_and(|&c| c != 0)
        && name.get(3).is_some_and(|&c| c != 0)
    {
        return termcap2key(name[2], name[3]);
    }

    let mut end = 0;
    while name.get(end).is_some_and(|&c| crate::ascii_defs::ascii_isident(i32::from(c))) {
        end += 1;
    }
    let candidate = &name[..end];

    for entry in KEY_NAMES_TABLE {
        if candidate.eq_ignore_ascii_case(entry.name.as_bytes()) {
            return entry.key;
        }
    }
    0
}

/// Try to include modifiers (except alt/meta) in the key. Changes
/// `"Shift-a"` to `'A'`, `"Ctrl-@"` to `<Nul>`, etc. (`extract_modifiers`).
///
/// `simplify`: if `false`, don't do Ctrl. `did_simplify`: set to `true`
/// when it is `Some` and `simplify` is `true` and Ctrl is removed from
/// `modifiers`.
///
/// No real caller is translated yet (`find_special_key`, needing
/// substantially more parsing logic beyond what's translated so far -
/// see this module's own doc comment) - harvested ahead of it,
/// matching this crate's established precedent for a small,
/// self-contained function with no design freedom of its own.
#[must_use]
#[allow(dead_code)] // no real translated caller yet - see this function's own doc comment
pub fn extract_modifiers(key: i32, modifiers: &mut i32, simplify: bool, did_simplify: Option<&mut bool>) -> i32 {
    let mut key = key;

    if *modifiers & i32::from(crate::keycodes_defs::MOD_MASK_SHIFT) != 0
        && crate::macros_defs::ascii_isalpha(key)
    {
        key = crate::macros_defs::toupper_asc(key);
        // With <C-S-a> we keep the shift modifier.
        // With <S-a>, <A-S-a> and <S-A> we don't keep the shift modifier.
        if *modifiers & i32::from(crate::keycodes_defs::MOD_MASK_CTRL) == 0 {
            *modifiers &= !i32::from(crate::keycodes_defs::MOD_MASK_SHIFT);
        }
    }

    // <C-H> and <C-h> mean the same thing, always use "H"
    if *modifiers & i32::from(crate::keycodes_defs::MOD_MASK_CTRL) != 0
        && crate::macros_defs::ascii_isalpha(key)
    {
        key = crate::macros_defs::toupper_asc(key);
    }

    if simplify
        && *modifiers & i32::from(crate::keycodes_defs::MOD_MASK_CTRL) != 0
        && ((key >= i32::from(b'?') && key <= i32::from(b'_')) || crate::macros_defs::ascii_isalpha(key))
    {
        key = crate::ascii_defs::ctrl_chr(key);
        *modifiers &= !i32::from(crate::keycodes_defs::MOD_MASK_CTRL);
        if key == 0 {
            // <C-@> is <Nul>
            key = K_ZERO;
        }
        if let Some(did_simplify) = did_simplify {
            *did_simplify = true;
        }
    }

    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_mouse_button_decodes_click_drag_release_and_unknown() {
        assert_eq!(
            get_mouse_button(i32::from(crate::keycodes_defs::KE_LEFTMOUSE)),
            (crate::mouse::MOUSE_LEFT, true, false)
        );
        assert_eq!(
            get_mouse_button(i32::from(crate::keycodes_defs::KE_X2DRAG)),
            (crate::mouse::MOUSE_X2, false, true)
        );
        assert_eq!(
            get_mouse_button(i32::from(crate::keycodes_defs::KE_IGNORE)),
            (crate::mouse::MOUSE_RELEASE, false, false)
        );
        assert_eq!(get_mouse_button(-999), (0, false, false));
    }

    #[test]
    fn name_to_mod_mask_recognizes_every_letter() {
        assert_eq!(name_to_mod_mask('S' as i32), i32::from(crate::keycodes_defs::MOD_MASK_SHIFT));
        assert_eq!(name_to_mod_mask('C' as i32), i32::from(crate::keycodes_defs::MOD_MASK_CTRL));
        assert_eq!(name_to_mod_mask('M' as i32), i32::from(crate::keycodes_defs::MOD_MASK_ALT));
        assert_eq!(name_to_mod_mask('T' as i32), i32::from(crate::keycodes_defs::MOD_MASK_META));
        assert_eq!(name_to_mod_mask('D' as i32), i32::from(crate::keycodes_defs::MOD_MASK_CMD));
        assert_eq!(name_to_mod_mask('2' as i32), i32::from(crate::keycodes_defs::MOD_MASK_2CLICK));
        assert_eq!(name_to_mod_mask('3' as i32), i32::from(crate::keycodes_defs::MOD_MASK_3CLICK));
        assert_eq!(name_to_mod_mask('4' as i32), i32::from(crate::keycodes_defs::MOD_MASK_4CLICK));
        // 'A' is a second, later entry for MOD_MASK_ALT.
        assert_eq!(name_to_mod_mask('A' as i32), i32::from(crate::keycodes_defs::MOD_MASK_ALT));
    }

    #[test]
    fn name_to_mod_mask_is_case_insensitive() {
        assert_eq!(name_to_mod_mask('s' as i32), name_to_mod_mask('S' as i32));
        assert_eq!(name_to_mod_mask('c' as i32), name_to_mod_mask('C' as i32));
    }

    #[test]
    fn name_to_mod_mask_unknown_letter_is_zero() {
        assert_eq!(name_to_mod_mask('Q' as i32), 0);
        assert_eq!(name_to_mod_mask('1' as i32), 0);
    }

    #[test]
    fn handle_x_keys_maps_arrow_keys() {
        assert_eq!(handle_x_keys(K_XUP), K_UP);
        assert_eq!(handle_x_keys(K_XDOWN), K_DOWN);
        assert_eq!(handle_x_keys(K_XLEFT), K_LEFT);
        assert_eq!(handle_x_keys(K_XRIGHT), K_RIGHT);
    }

    #[test]
    fn handle_x_keys_maps_both_home_variants() {
        assert_eq!(handle_x_keys(K_XHOME), K_HOME);
        assert_eq!(handle_x_keys(K_ZHOME), K_HOME);
    }

    #[test]
    fn handle_x_keys_maps_both_end_variants() {
        assert_eq!(handle_x_keys(K_XEND), K_END);
        assert_eq!(handle_x_keys(K_ZEND), K_END);
    }

    #[test]
    fn handle_x_keys_maps_function_keys_and_shifted_variants() {
        assert_eq!(handle_x_keys(K_XF1), K_F1);
        assert_eq!(handle_x_keys(K_XF2), K_F2);
        assert_eq!(handle_x_keys(K_XF3), K_F3);
        assert_eq!(handle_x_keys(K_XF4), K_F4);
        assert_eq!(handle_x_keys(K_S_XF1), K_S_F1);
        assert_eq!(handle_x_keys(K_S_XF2), K_S_F2);
        assert_eq!(handle_x_keys(K_S_XF3), K_S_F3);
        assert_eq!(handle_x_keys(K_S_XF4), K_S_F4);
    }

    #[test]
    fn handle_x_keys_leaves_unrelated_keys_unchanged() {
        assert_eq!(handle_x_keys(K_UP), K_UP);
        assert_eq!(handle_x_keys(42), 42);
    }

    #[test]
    fn simplify_key_returns_key_unchanged_without_shift_or_ctrl() {
        let mut modifiers = i32::from(crate::keycodes_defs::MOD_MASK_ALT);
        assert_eq!(simplify_key(K_UP, &mut modifiers), K_UP);
        assert_eq!(modifiers, i32::from(crate::keycodes_defs::MOD_MASK_ALT));
    }

    #[test]
    fn simplify_key_tab_plus_shift_is_a_special_case() {
        let mut modifiers = i32::from(MOD_MASK_SHIFT);
        assert_eq!(simplify_key(i32::from(TAB), &mut modifiers), crate::keycodes_defs::K_S_TAB);
        // The Shift bit is consumed - now folded into K_S_TAB itself.
        assert_eq!(modifiers, 0);
    }

    #[test]
    fn simplify_key_folds_ctrl_left_arrow() {
        let mut modifiers = i32::from(MOD_MASK_CTRL);
        assert_eq!(simplify_key(K_LEFT, &mut modifiers), crate::keycodes_defs::K_C_LEFT);
        assert_eq!(modifiers, 0);
    }

    #[test]
    fn simplify_key_folds_shift_up_arrow() {
        let mut modifiers = i32::from(MOD_MASK_SHIFT);
        assert_eq!(simplify_key(K_UP, &mut modifiers), crate::keycodes_defs::K_S_UP);
        assert_eq!(modifiers, 0);
    }

    #[test]
    fn simplify_key_preserves_other_modifier_bits_when_folding() {
        // ALT should survive being combined with a Ctrl-Left fold.
        let mut modifiers = i32::from(MOD_MASK_CTRL) | i32::from(crate::keycodes_defs::MOD_MASK_ALT);
        assert_eq!(simplify_key(K_LEFT, &mut modifiers), crate::keycodes_defs::K_C_LEFT);
        assert_eq!(modifiers, i32::from(crate::keycodes_defs::MOD_MASK_ALT));
    }

    #[test]
    fn simplify_key_no_matching_table_entry_leaves_everything_unchanged() {
        // K_F1 (a plain, un-simplifiable function key with no combined
        // Ctrl form in the table) with Ctrl set: no match, nothing
        // changes.
        let mut modifiers = i32::from(MOD_MASK_CTRL);
        assert_eq!(simplify_key(K_F1, &mut modifiers), K_F1);
        assert_eq!(modifiers, i32::from(MOD_MASK_CTRL));
    }

    // --- vim_unescape_ks ---

    fn ks() -> (u8, u8, u8) {
        (crate::keycodes_defs::K_SPECIAL, crate::keycodes_defs::KS_SPECIAL, crate::keycodes_defs::KE_FILLER)
    }

    #[test]
    fn vim_unescape_ks_unescapes_a_single_sequence() {
        let (k, ks, ke) = ks();
        let mut buf = [k, ks, ke, b'a', b'b'];
        let new_len = vim_unescape_ks(&mut buf);
        assert_eq!(new_len, 3);
        assert_eq!(&buf[..new_len], &[k, b'a', b'b']);
    }

    #[test]
    fn vim_unescape_ks_no_escape_sequences_is_unchanged() {
        let mut buf = *b"hi\0\0\0";
        let new_len = vim_unescape_ks(&mut buf);
        assert_eq!(new_len, 2);
        assert_eq!(&buf[..new_len], b"hi");
    }

    #[test]
    fn vim_unescape_ks_bare_escape_sequence_becomes_one_byte() {
        let (k, ks, ke) = ks();
        let mut buf = [k, ks, ke];
        let new_len = vim_unescape_ks(&mut buf);
        assert_eq!(new_len, 1);
        assert_eq!(buf[0], k);
    }

    #[test]
    fn vim_unescape_ks_k_special_not_followed_by_the_full_pattern_is_left_alone() {
        // K_SPECIAL followed by bytes that don't match KS_SPECIAL/
        // KE_FILLER exactly - not a real escape sequence, copied
        // through unchanged (matching the original's own exact-match
        // requirement on all 3 bytes).
        let (k, _, _) = ks();
        let mut buf = [k, b'x', b'y', 0];
        let new_len = vim_unescape_ks(&mut buf);
        assert_eq!(new_len, 3);
        assert_eq!(&buf[..new_len], &[k, b'x', b'y']);
    }

    #[test]
    fn vim_unescape_ks_multiple_sequences_in_one_buffer() {
        let (k, ks, ke) = ks();
        let mut buf = [k, ks, ke, b'-', k, ks, ke, 0];
        let new_len = vim_unescape_ks(&mut buf);
        assert_eq!(new_len, 3);
        assert_eq!(&buf[..new_len], &[k, b'-', k]);
    }

    #[test]
    fn vim_unescape_ks_empty_string_stays_empty() {
        let mut buf = [0u8];
        assert_eq!(vim_unescape_ks(&mut buf), 0);
    }

    // --- add_char2buf ---

    #[test]
    fn add_char2buf_ascii_character_writes_one_byte_unescaped() {
        let mut buf = [0u8; 8];
        let written = add_char2buf('A' as i32, &mut buf);
        assert_eq!(written, 1);
        assert_eq!(&buf[..written], &[0x41]);
    }

    #[test]
    fn special_to_buf_encodes_modifiers_special_keys_and_characters() {
        assert_eq!(
            special_to_buf(
                crate::keycodes_defs::K_UP,
                i32::from(crate::keycodes_defs::MOD_MASK_SHIFT),
                false,
            ),
            vec![
                crate::keycodes_defs::K_SPECIAL,
                crate::keycodes_defs::KS_MODIFIER,
                crate::keycodes_defs::MOD_MASK_SHIFT as u8,
                crate::keycodes_defs::K_SPECIAL,
                b'k',
                b'u',
            ]
        );
        assert_eq!(special_to_buf(i32::from(b'A'), 0, false), b"A");
        assert_eq!(special_to_buf(0x00e9, 0, false), "é".as_bytes());
        assert_eq!(
            special_to_buf(0x80, 0, true),
            vec![
                0xc2,
                crate::keycodes_defs::K_SPECIAL,
                crate::keycodes_defs::KS_SPECIAL,
                crate::keycodes_defs::KE_FILLER,
            ]
        );
    }

    #[test]
    fn add_char2buf_two_byte_character_with_no_k_special_byte() {
        // 'e' with acute accent (U+00E9) -> UTF-8 [0xC3, 0xA9], neither
        // byte equals K_SPECIAL (0x80), so no escaping happens.
        let mut buf = [0u8; 8];
        let written = add_char2buf(0xE9, &mut buf);
        assert_eq!(written, 2);
        assert_eq!(&buf[..written], &[0xC3, 0xA9]);
    }

    #[test]
    fn add_char2buf_escapes_a_k_special_byte_within_the_encoding() {
        // U+0080 -> UTF-8 [0xC2, 0x80]. The second byte (0x80) equals
        // K_SPECIAL, so it expands into the 3-byte escape sequence;
        // the first byte (0xC2) passes through unescaped.
        let (k, ks, ke) = ks();
        let mut buf = [0u8; 8];
        let written = add_char2buf(0x80, &mut buf);
        assert_eq!(written, 4);
        assert_eq!(&buf[..written], &[0xC2, k, ks, ke]);
    }

    // --- vim_strsave_escape_ks ---

    #[test]
    fn vim_strsave_escape_ks_plain_ascii_is_unchanged() {
        assert_eq!(vim_strsave_escape_ks(b"abc"), b"abc".to_vec());
    }

    #[test]
    fn vim_strsave_escape_ks_copies_an_existing_escaped_sequence_unmodified() {
        let (k, ks, ke) = ks();
        let input = [k, ks, ke, b'a'];
        assert_eq!(vim_strsave_escape_ks(&input), vec![k, ks, ke, b'a']);
    }

    #[test]
    fn vim_strsave_escape_ks_encodes_a_lone_trailing_k_special_byte_via_roundtrip() {
        // A trailing, ISOLATED 0x80 byte (fewer than 2 bytes follow,
        // so the "already escaped" check fails) falls through to
        // utf_ptr2char's own "illegal byte returns itself" fallback
        // (128), which add_char2buf then re-encodes as valid UTF-8
        // (U+0080 -> [0xC2, 0x80]) and escapes the resulting
        // K_SPECIAL byte - matching the original's own real, if
        // unusual, "be careful, it can be an illegal byte!" behavior.
        let (k, ks, ke) = ks();
        let input = [b'a', k];
        assert_eq!(vim_strsave_escape_ks(&input), vec![b'a', 0xC2, k, ks, ke]);
    }

    #[test]
    fn vim_strsave_escape_ks_truncates_at_the_first_embedded_nul() {
        let input = *b"ab\0cd";
        assert_eq!(vim_strsave_escape_ks(&input), b"ab".to_vec());
    }

    #[test]
    fn vim_strsave_escape_ks_empty_input_returns_empty() {
        assert_eq!(vim_strsave_escape_ks(b""), Vec::<u8>::new());
    }

    // --- KEY_NAMES_TABLE / find_special_key_in_table / get_special_key_code ---

    #[test]
    fn key_names_table_has_the_expected_entry_count() {
        // Mechanically transcribed from a 187-entry pre-built copy of
        // keycode_names.generated.h - hand-counted via the same
        // extraction script used to generate the table itself.
        assert_eq!(KEY_NAMES_TABLE.len(), 187);
    }

    #[test]
    fn find_special_key_in_table_finds_the_canonical_non_alt_entry() {
        let idx = find_special_key_in_table(K_UP);
        assert!(idx >= 0);
        assert_eq!(KEY_NAMES_TABLE[idx as usize].name, "Up");
        assert!(!KEY_NAMES_TABLE[idx as usize].is_alt);
    }

    #[test]
    fn find_special_key_in_table_skips_an_alt_only_match_to_find_the_canonical_one() {
        // K_KUP has TWO table entries: an alt "KP8" and the canonical,
        // non-alt "kUp" - the function must return the LATTER.
        let idx = find_special_key_in_table(K_KUP);
        assert!(idx >= 0);
        assert_eq!(KEY_NAMES_TABLE[idx as usize].name, "kUp");
        assert!(!KEY_NAMES_TABLE[idx as usize].is_alt);
    }

    #[test]
    fn find_special_key_in_table_returns_minus_1_for_an_unrecognized_key() {
        assert_eq!(find_special_key_in_table(999_999), -1);
    }

    #[test]
    fn get_special_key_code_finds_a_plain_name() {
        assert_eq!(get_special_key_code(b"Up"), K_UP);
        assert_eq!(get_special_key_code(b"F1"), K_F1);
    }

    #[test]
    fn get_special_key_code_is_case_insensitive() {
        assert_eq!(get_special_key_code(b"up"), K_UP);
        assert_eq!(get_special_key_code(b"UP"), K_UP);
        assert_eq!(get_special_key_code(b"uP"), K_UP);
        assert_eq!(get_special_key_code(b"f1"), K_F1);
    }

    #[test]
    fn get_special_key_code_finds_an_alt_name_too() {
        // "KP8" is K_KUP's own alt-only name - unlike
        // find_special_key_in_table, name-based lookup must still find
        // it (a real key notation like "<KP8>" must resolve).
        assert_eq!(get_special_key_code(b"KP8"), K_KUP);
    }

    #[test]
    fn get_special_key_code_stops_at_the_first_non_identifier_byte() {
        // Trailing, unrelated bytes after the name itself (e.g. a
        // closing '>' from "<Up>" notation, already stripped by a real
        // caller, or just garbage) don't prevent a match.
        assert_eq!(get_special_key_code(b"Up>"), K_UP);
        assert_eq!(get_special_key_code(b"Up rest"), K_UP);
    }

    #[test]
    fn get_special_key_code_unrecognized_name_is_zero() {
        assert_eq!(get_special_key_code(b"NotARealKeyName"), 0);
    }

    #[test]
    fn get_special_key_code_empty_name_is_zero() {
        assert_eq!(get_special_key_code(b""), 0);
    }

    #[test]
    fn get_special_key_code_termcap_form() {
        // "t_ab" bypasses the table entirely: TERMCAP2KEY('a', 'b').
        assert_eq!(get_special_key_code(b"t_ab"), termcap2key(b'a', b'b'));
        // Extra trailing bytes past the 2 termcap bytes are ignored.
        assert_eq!(get_special_key_code(b"t_abXYZ"), termcap2key(b'a', b'b'));
    }

    #[test]
    fn get_special_key_code_termcap_form_needs_at_least_2_bytes_after_t_underscore() {
        // "t_a" has a 3rd byte but no 4th - not a valid termcap form,
        // falls through to the ordinary identifier-bounded table scan
        // instead (and "t_a" itself isn't a real key name, so this
        // resolves to 0).
        assert_eq!(get_special_key_code(b"t_a"), 0);
    }

    // --- find_special_key ---

    #[test]
    fn find_special_key_parses_a_named_key_and_reports_consumed_bytes() {
        assert_eq!(
            find_special_key(b"<Up>tail", crate::keycodes_defs::fsk::KEYCODE),
            Some((K_UP, 0, 4, false))
        );
    }

    #[test]
    fn find_special_key_parses_and_simplifies_modifiers() {
        assert_eq!(
            find_special_key(
                b"<C-A>",
                crate::keycodes_defs::fsk::KEYCODE | crate::keycodes_defs::fsk::SIMPLIFY,
            ),
            Some((i32::from(crate::ascii_defs::CTRL_A), 0, 5, true))
        );
        assert_eq!(
            find_special_key(
                b"<S-a>",
                crate::keycodes_defs::fsk::KEYCODE | crate::keycodes_defs::fsk::SIMPLIFY,
            ),
            Some((i32::from(b'A'), 0, 5, false))
        );
    }

    #[test]
    fn find_special_key_keeps_alt_as_a_modifier() {
        assert_eq!(
            find_special_key(
                b"<M-a>",
                crate::keycodes_defs::fsk::KEYCODE | crate::keycodes_defs::fsk::SIMPLIFY,
            ),
            Some((
                i32::from(b'a'),
                i32::from(crate::keycodes_defs::MOD_MASK_ALT),
                5,
                false,
            ))
        );
    }

    #[test]
    fn find_special_key_parses_char_and_termcap_forms() {
        assert_eq!(
            find_special_key(b"<Char-0x41>", crate::keycodes_defs::fsk::KEYCODE),
            Some((i32::from(b'A'), 0, 11, false))
        );
        assert_eq!(
            find_special_key(b"<t_ab>", crate::keycodes_defs::fsk::KEYCODE),
            Some((termcap2key(b'a', b'b'), 0, 6, false))
        );
    }

    #[test]
    fn find_special_key_maps_delete_to_a_byte_without_keycode_flag() {
        assert_eq!(
            find_special_key(b"<Del>", 0),
            Some((i32::from(crate::ascii_defs::DEL), 0, 5, false))
        );
    }

    #[test]
    fn find_special_key_rejects_malformed_and_unknown_names() {
        assert_eq!(find_special_key(b"Up", 0), None);
        assert_eq!(find_special_key(b"<Up", 0), None);
        assert_eq!(find_special_key(b"<NotARealKey>", 0), None);
        assert_eq!(find_special_key(b"<Q-Up>", 0), None);
    }

    // --- get_special_key_name ---

    #[test]
    fn get_special_key_name_formats_named_and_shifted_keys() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { get_special_key_name(K_UP, 0) }, b"<Up>");
        assert_eq!(
            unsafe { get_special_key_name(crate::keycodes_defs::K_S_UP, 0) },
            b"<S-Up>"
        );
    }

    #[test]
    fn get_special_key_name_extracts_control_and_alt_modifiers() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(
            unsafe { get_special_key_name(i32::from(crate::ascii_defs::CTRL_A), 0) },
            b"<C-A>"
        );
        assert_eq!(
            unsafe {
                get_special_key_name(
                    i32::from(b'a'),
                    i32::from(crate::keycodes_defs::MOD_MASK_ALT),
                )
            },
            b"<M-a>"
        );
    }

    #[test]
    fn get_special_key_name_formats_unknown_termcap_and_multibyte_keys() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(
            unsafe { get_special_key_name(termcap2key(b'a', b'b'), 0) },
            b"<t_ab>"
        );
        assert_eq!(
            unsafe { get_special_key_name('é' as i32, 0) },
            "<é>".as_bytes()
        );
    }

    // --- extract_modifiers ---

    #[test]
    fn extract_modifiers_shift_alone_uppercases_and_clears_shift() {
        let mut modifiers = i32::from(crate::keycodes_defs::MOD_MASK_SHIFT);
        let key = extract_modifiers(i32::from(b'a'), &mut modifiers, true, None);
        assert_eq!(key, i32::from(b'A'));
        assert_eq!(modifiers, 0);
    }

    #[test]
    fn extract_modifiers_ctrl_shift_together_keeps_shift_and_simplifies_to_ctrl_a() {
        let mut modifiers =
            i32::from(crate::keycodes_defs::MOD_MASK_SHIFT) | i32::from(crate::keycodes_defs::MOD_MASK_CTRL);
        let mut did_simplify = false;
        let key = extract_modifiers(i32::from(b'a'), &mut modifiers, true, Some(&mut did_simplify));
        // Ctrl-A is 0x01.
        assert_eq!(key, 0x01);
        // Shift is retained (only Ctrl was consumed).
        assert_eq!(modifiers, i32::from(crate::keycodes_defs::MOD_MASK_SHIFT));
        assert!(did_simplify);
    }

    #[test]
    fn extract_modifiers_ctrl_h_simplifies_to_backspace() {
        let mut modifiers = i32::from(crate::keycodes_defs::MOD_MASK_CTRL);
        let key = extract_modifiers(i32::from(b'h'), &mut modifiers, true, None);
        assert_eq!(key, i32::from(crate::ascii_defs::BS));
        assert_eq!(modifiers, 0);
    }

    #[test]
    fn extract_modifiers_ctrl_at_becomes_k_zero() {
        // <C-@> is <Nul> - ctrl_chr('@') computes to 0, which is
        // special-cased to K_ZERO instead of a bare 0.
        let mut modifiers = i32::from(crate::keycodes_defs::MOD_MASK_CTRL);
        let mut did_simplify = false;
        let key = extract_modifiers(i32::from(b'@'), &mut modifiers, true, Some(&mut did_simplify));
        assert_eq!(key, crate::keycodes_defs::K_ZERO);
        assert_eq!(modifiers, 0);
        assert!(did_simplify);
    }

    #[test]
    fn extract_modifiers_simplify_false_leaves_ctrl_modifier_untouched() {
        let mut modifiers = i32::from(crate::keycodes_defs::MOD_MASK_CTRL);
        let key = extract_modifiers(i32::from(b'h'), &mut modifiers, false, None);
        // Still uppercased (that part isn't gated on `simplify`), but
        // the Ctrl bit itself is NOT folded into the key.
        assert_eq!(key, i32::from(b'H'));
        assert_eq!(modifiers, i32::from(crate::keycodes_defs::MOD_MASK_CTRL));
    }

    #[test]
    fn extract_modifiers_ctrl_on_a_digit_outside_the_special_range_is_unaffected() {
        // '5' is neither alphabetic nor in the '?'..='_' range, so Ctrl
        // is never folded into it.
        let mut modifiers = i32::from(crate::keycodes_defs::MOD_MASK_CTRL);
        let key = extract_modifiers(i32::from(b'5'), &mut modifiers, true, None);
        assert_eq!(key, i32::from(b'5'));
        assert_eq!(modifiers, i32::from(crate::keycodes_defs::MOD_MASK_CTRL));
    }

    #[test]
    fn extract_modifiers_no_relevant_modifiers_leaves_key_and_modifiers_unchanged() {
        let mut modifiers = i32::from(crate::keycodes_defs::MOD_MASK_ALT);
        let key = extract_modifiers(i32::from(b'a'), &mut modifiers, true, None);
        assert_eq!(key, i32::from(b'a'));
        assert_eq!(modifiers, i32::from(crate::keycodes_defs::MOD_MASK_ALT));
    }

    #[test]
    fn extract_modifiers_did_simplify_none_does_not_panic_when_simplification_happens() {
        let mut modifiers = i32::from(crate::keycodes_defs::MOD_MASK_CTRL);
        // Should not panic even though the Ctrl-simplification branch
        // is genuinely taken here.
        let key = extract_modifiers(i32::from(b'h'), &mut modifiers, true, None);
        assert_eq!(key, i32::from(crate::ascii_defs::BS));
    }
}
