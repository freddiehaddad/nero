//! `src/nvim/api/win_config.c` - floating/split window configuration.
//!
//! Only the border-style parsing half of this file is translated so far:
//! `parse_border_style` and `parse_winborder`, which `optionstr.c`'s
//! `did_set_winborder`/`did_set_pumborder` need in order to validate
//! `'winborder'`/`'pumborder'`.
//!
//! Deferred (each genuinely blocked, not simply "not gotten to yet"):
//!
//! `nvim_open_win`/`nvim_win_set_config`/`nvim_win_get_config` and their
//! own `parse_win_config`/`apply_window_border`/`win_config_float`
//! helpers need real window creation/splitting (`win_split_ins`,
//! `win_new_float`), the redraw pipeline, and the `Dict(win_config)`
//! keyset-decoding machinery - none translated.
//!
//! Border-style parsing includes two-item `["char", "HlGroup"]`
//! entries and the built-in `"shadow"` style's
//! `FloatShadow`/`FloatShadowThrough` groups through the real
//! highlight registry.

use crate::api::private::defs::{Error, ErrorType, Object};
use crate::buffer_defs::WinConfig;
use crate::option_vars::OPT_WINBORDER_VALUES;
use crate::types_defs::MAX_SCHAR_SIZE;

/// One entry of `parse_border_style`'s own `defaults[]` table: a style
/// name plus the eight border characters it expands to, clockwise from
/// the top-left corner.
///
/// `shadow_color` marks the one style (`"shadow"`) whose entry also
/// assigns highlight ids - see `parse_border_style`'s own doc comment.
struct BorderDefault {
    name: &'static str,
    chars: [&'static str; 8],
    shadow_color: bool,
}

/// `defaults[]` from `parse_border_style`.
///
/// The original indexes `opt_winborder_values[1]`..`[6]` for the names;
/// those are `"double"`, `"single"`, `"shadow"`, `"rounded"`, `"solid"`
/// and `"bold"` respectively. They are referenced through the same
/// shared table here rather than duplicated as literals, so the two can
/// never drift apart.
const BORDER_DEFAULTS: [BorderDefault; 6] = [
    BorderDefault {
        name: OPT_WINBORDER_VALUES[1],
        chars: ["╔", "═", "╗", "║", "╝", "═", "╚", "║"],
        shadow_color: false,
    },
    BorderDefault {
        name: OPT_WINBORDER_VALUES[2],
        chars: ["┌", "─", "┐", "│", "┘", "─", "└", "│"],
        shadow_color: false,
    },
    BorderDefault {
        name: OPT_WINBORDER_VALUES[3],
        chars: ["", "", " ", " ", " ", " ", " ", ""],
        shadow_color: true,
    },
    BorderDefault {
        name: OPT_WINBORDER_VALUES[4],
        chars: ["╭", "─", "╮", "│", "╯", "─", "╰", "│"],
        shadow_color: false,
    },
    BorderDefault {
        name: OPT_WINBORDER_VALUES[5],
        chars: [" ", " ", " ", " ", " ", " ", " ", " "],
        shadow_color: false,
    },
    BorderDefault {
        name: OPT_WINBORDER_VALUES[6],
        chars: ["┏", "━", "┓", "┃", "┛", "━", "┗", "┃"],
        shadow_color: false,
    },
];

/// Copy `s`'s bytes into one `border_chars` slot, truncating to the
/// slot's own capacity and NUL-terminating, exactly as the original's
/// `memcpy(chars[i], string.data, len); chars[i][len] = NUL;` does.
fn set_border_char(slot: &mut [u8; MAX_SCHAR_SIZE], s: &[u8]) {
    let len = s.len().min(MAX_SCHAR_SIZE - 1);
    slot[..len].copy_from_slice(&s[..len]);
    slot[len] = 0;
}

/// `parse_border_style(Object style, WinConfig *fconfig, Error *err)`
///
/// Fills `fconfig`'s border characters and highlight ids from either a
/// style name (`Object::String`) or an explicit list of border
/// characters (`Object::Array`). Any other object type is ignored
/// entirely (the original's own `if`/`else if` chain has no final
/// `else`), leaving `fconfig->border` set to `true` and everything else
/// untouched.
///
pub fn parse_border_style(style: &Object, fconfig: &mut WinConfig, err: &mut Error) {
    fconfig.border = true;

    match style {
        Object::Array(arr) => {
            let mut size = arr.len();
            // Must be 1, 2, 4 or 8 - i.e. a non-zero power of two that
            // is at most 8, matching `!size || size > 8 ||
            // (size & (size - 1))`.
            if size == 0 || size > 8 || (size & (size - 1)) != 0 {
                err.r#type = ErrorType::Validation;
                err.msg = Some("Invalid 'border': expected 1, 2, 4, or 8 chars".to_string());
                return;
            }
            for (i, item) in arr.iter().enumerate() {
                let (string, hl_id) = match item {
                    Object::String(s) => (s, 0),
                    Object::Array(item) => {
                        if item.is_empty() || item.len() > 2 {
                            err.r#type = ErrorType::Validation;
                            err.msg =
                                Some("Invalid 'border': expected 1 or 2-item Array".to_string());
                            return;
                        }
                        let Object::String(string) = &item[0] else {
                            err.r#type = ErrorType::Validation;
                            err.msg =
                                Some("Invalid 'border': expected Array of Strings".to_string());
                            return;
                        };
                        let hl_id = if item.len() == 2 {
                            let id = unsafe {
                                crate::api::private::helpers::object_to_hl_id(
                                    &item[1],
                                    "border char highlight",
                                    err,
                                )
                            };
                            if err.is_set() {
                                return;
                            }
                            id
                        } else {
                            0
                        };
                        (string, hl_id)
                    }
                    other => {
                        err.r#type = ErrorType::Validation;
                        err.msg = Some(format!(
                            "Invalid 'border': expected String or Array, got {:?}",
                            other.object_type()
                        ));
                        return;
                    }
                };
                // SAFETY: `mb_string2cells` only reads `string`'s own
                // bytes; a border character is arbitrary user text.
                if !string.is_empty() && unsafe { crate::mbyte::mb_string2cells(string) } > 1 {
                    err.r#type = ErrorType::Validation;
                    err.msg = Some("Invalid 'border': expected only one-cell chars".to_string());
                    return;
                }
                set_border_char(&mut fconfig.border_chars[i], string);
                fconfig.border_hl_ids[i] = hl_id;
            }
            // Repeat the given chars until all eight slots are filled.
            while size < 8 {
                fconfig.border_chars.copy_within(0..size, size);
                fconfig.border_hl_ids.copy_within(0..size, size);
                size <<= 1;
            }
            let c = &fconfig.border_chars;
            let present = |i: usize| c[i][0] != 0;
            if (present(7) && present(1) && !present(0))
                || (present(1) && present(3) && !present(2))
                || (present(3) && present(5) && !present(4))
                || (present(5) && present(7) && !present(6))
            {
                err.r#type = ErrorType::Validation;
                err.msg =
                    Some("Invalid 'border': expected corner char between edge chars".to_string());
            }
        }
        Object::String(str) => {
            if str.is_empty() || str.as_slice() == b"none" {
                fconfig.border = false;
                // border text does not work with border equal none
                fconfig.title = false;
                fconfig.footer = false;
                return;
            }
            for d in &BORDER_DEFAULTS {
                if str.as_slice() == d.name.as_bytes() {
                    for (slot, ch) in fconfig.border_chars.iter_mut().zip(d.chars.iter()) {
                        set_border_char(slot, ch.as_bytes());
                    }
                    fconfig.border_hl_ids = [0; 8];
                    if d.shadow_color {
                        let blend = unsafe {
                            crate::highlight_group::syn_check_group(b"FloatShadow")
                        };
                        let through = unsafe {
                            crate::highlight_group::syn_check_group(
                                b"FloatShadowThrough",
                            )
                        };
                        fconfig.border_hl_ids[2] = through;
                        fconfig.border_hl_ids[3] = blend;
                        fconfig.border_hl_ids[4] = blend;
                        fconfig.border_hl_ids[5] = blend;
                        fconfig.border_hl_ids[6] = through;
                    }
                    return;
                }
            }
            err.r#type = ErrorType::Validation;
            err.msg = Some(format!(
                "Invalid 'border': '{}'",
                std::string::String::from_utf8_lossy(str)
            ));
        }
        _ => {}
    }
}

/// `parse_winborder(WinConfig *fconfig, char *border_opt, Error *err)`
///
/// Parses a border style name, or a custom comma-separated style, into
/// `fconfig`. Returns whether it was accepted.
///
/// A comma anywhere in `border_opt` selects the custom form: the value
/// is split on every comma and must yield exactly eight parts (the
/// original bails out early once a ninth is seen, which this translation
/// reproduces by refusing any split of more than eight parts). Anything
/// without a comma is treated as a style name.
///
/// The original's `if (!fconfig) return false;` null check has no Rust
/// equivalent - `fconfig` is a `&mut`, which is never null.
pub fn parse_winborder(fconfig: &mut WinConfig, border_opt: &[u8], err: &mut Error) -> bool {
    let style = if border_opt.contains(&b',') {
        let parts: Vec<&[u8]> = border_opt.split(|&b| b == b',').collect();
        if parts.len() != 8 {
            return false;
        }
        Object::Array(parts.iter().map(|p| Object::String(p.to_vec())).collect())
    } else {
        Object::String(border_opt.to_vec())
    };

    parse_border_style(&style, fconfig, err);
    !err.is_set()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BorderHighlightGuard {
        items: Vec<crate::highlight_group::HlGroup>,
        names: crate::map::Map<Vec<u8>, i32>,
    }

    impl BorderHighlightGuard {
        fn empty() -> Self {
            let table = unsafe { crate::highlight_group::HL_TABLE.get_mut() };
            let items = std::mem::take(&mut table.items);
            let names = std::mem::replace(
                unsafe { crate::highlight_group::HIGHLIGHT_UNAMES.get_mut() },
                crate::map::Map::new(),
            );
            Self { items, names }
        }
    }

    impl Drop for BorderHighlightGuard {
        fn drop(&mut self) {
            unsafe { crate::highlight_group::HL_TABLE.get_mut() }.items =
                std::mem::take(&mut self.items);
            *unsafe { crate::highlight_group::HIGHLIGHT_UNAMES.get_mut() } =
                std::mem::replace(&mut self.names, crate::map::Map::new());
        }
    }

    /// Read one border slot back as a byte slice, stopping at its NUL.
    fn slot(fc: &WinConfig, i: usize) -> &[u8] {
        let s = &fc.border_chars[i];
        let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
        &s[..end]
    }

    #[test]
    fn parse_border_style_none_and_empty_clear_the_border() {
        for v in [&b""[..], &b"none"[..]] {
            let mut fc = WinConfig {
                title: true,
                footer: true,
                ..Default::default()
            };
            let mut err = Error::default();
            parse_border_style(&Object::String(v.to_vec()), &mut fc, &mut err);
            assert!(!err.is_set());
            assert!(!fc.border);
            assert!(!fc.title);
            assert!(!fc.footer);
        }
    }

    #[test]
    fn parse_border_style_expands_each_named_style() {
        let _lock = crate::globals::global_state_test_lock();
        let _groups = BorderHighlightGuard::empty();
        // Spot-check every name plus its own first and last character,
        // hand-read straight off the original's `defaults[]` table.
        let cases: [(&str, &str, &str); 6] = [
            ("double", "╔", "║"),
            ("single", "┌", "│"),
            ("shadow", "", ""),
            ("rounded", "╭", "│"),
            ("solid", " ", " "),
            ("bold", "┏", "┃"),
        ];
        for (name, first, last) in cases {
            let mut fc = WinConfig::default();
            let mut err = Error::default();
            parse_border_style(
                &Object::String(name.as_bytes().to_vec()),
                &mut fc,
                &mut err,
            );
            assert!(!err.is_set(), "{name} should be accepted");
            assert!(fc.border);
            assert_eq!(slot(&fc, 0), first.as_bytes(), "{name} first char");
            assert_eq!(slot(&fc, 7), last.as_bytes(), "{name} last char");
            if name == "shadow" {
                assert!(fc.border_hl_ids[2] > 0);
                assert_eq!(fc.border_hl_ids[2], fc.border_hl_ids[6]);
                assert!(fc.border_hl_ids[3] > 0);
                assert_eq!(fc.border_hl_ids[3], fc.border_hl_ids[4]);
                assert_eq!(fc.border_hl_ids[4], fc.border_hl_ids[5]);
                assert_ne!(fc.border_hl_ids[2], fc.border_hl_ids[3]);
            } else {
                assert_eq!(fc.border_hl_ids, [0; 8]);
            }
        }
    }

    #[test]
    fn parse_border_style_accepts_char_and_highlight_pairs() {
        let _lock = crate::globals::global_state_test_lock();
        let _groups = BorderHighlightGuard::empty();
        let style = Object::Array(vec![Object::Array(vec![
            Object::String(b"|".to_vec()),
            Object::String(b"FloatBorder".to_vec()),
        ])]);
        let mut fc = WinConfig::default();
        let mut err = Error::default();

        parse_border_style(&style, &mut fc, &mut err);

        assert!(!err.is_set());
        assert!(fc.border_hl_ids[0] > 0);
        assert!(fc
            .border_hl_ids
            .iter()
            .all(|id| *id == fc.border_hl_ids[0]));
        for i in 0..8 {
            assert_eq!(slot(&fc, i), b"|");
        }
    }

    #[test]
    fn parse_border_style_rejects_bad_nested_array_shapes() {
        let _lock = crate::globals::global_state_test_lock();
        let _groups = BorderHighlightGuard::empty();
        for item in [
            Object::Array(Vec::new()),
            Object::Array(vec![
                Object::String(b"|".to_vec()),
                Object::String(b"Border".to_vec()),
                Object::String(b"Extra".to_vec()),
            ]),
            Object::Array(vec![Object::Integer(1)]),
            Object::Array(vec![
                Object::String(b"|".to_vec()),
                Object::Boolean(true),
            ]),
        ] {
            let mut fc = WinConfig::default();
            let mut err = Error::default();
            parse_border_style(&Object::Array(vec![item]), &mut fc, &mut err);
            assert!(err.is_set());
            assert_eq!(err.r#type, ErrorType::Validation);
        }
    }

    #[test]
    fn parse_border_style_rejects_an_unknown_name() {
        let mut fc = WinConfig::default();
        let mut err = Error::default();
        parse_border_style(&Object::String(b"nope".to_vec()), &mut fc, &mut err);
        assert!(err.is_set());
        assert_eq!(err.r#type, ErrorType::Validation);
    }

    #[test]
    fn parse_border_style_accepts_eight_explicit_chars() {
        let items: Vec<Object> = [b"1", b"2", b"3", b"4", b"5", b"6", b"7", b"8"]
            .iter()
            .map(|c| Object::String(c.to_vec()))
            .collect();
        let mut fc = WinConfig::default();
        let mut err = Error::default();
        parse_border_style(&Object::Array(items), &mut fc, &mut err);
        assert!(!err.is_set());
        for i in 0..8 {
            assert_eq!(slot(&fc, i), format!("{}", i + 1).as_bytes());
        }
    }

    #[test]
    fn parse_border_style_repeats_a_shorter_list_to_fill_eight_slots() {
        // A single char fills all eight; a two-char list alternates.
        let mut fc = WinConfig::default();
        let mut err = Error::default();
        parse_border_style(
            &Object::Array(vec![Object::String(b"x".to_vec())]),
            &mut fc,
            &mut err,
        );
        assert!(!err.is_set());
        for i in 0..8 {
            assert_eq!(slot(&fc, i), b"x");
        }

        let mut fc = WinConfig::default();
        let mut err = Error::default();
        parse_border_style(
            &Object::Array(vec![
                Object::String(b"a".to_vec()),
                Object::String(b"b".to_vec()),
            ]),
            &mut fc,
            &mut err,
        );
        assert!(!err.is_set());
        for i in 0..8 {
            assert_eq!(slot(&fc, i), if i % 2 == 0 { b"a" } else { b"b" });
        }
    }

    #[test]
    fn parse_border_style_rejects_a_bad_array_length() {
        for n in [0usize, 3, 5, 9] {
            let items: Vec<Object> = (0..n).map(|_| Object::String(b"x".to_vec())).collect();
            let mut fc = WinConfig::default();
            let mut err = Error::default();
            parse_border_style(&Object::Array(items), &mut fc, &mut err);
            assert!(err.is_set(), "{n} chars should be rejected");
        }
    }

    #[test]
    fn parse_border_style_rejects_a_double_width_char() {
        let items: Vec<Object> = (0..8).map(|_| Object::String("一".into())).collect();
        let mut fc = WinConfig::default();
        let mut err = Error::default();
        parse_border_style(&Object::Array(items), &mut fc, &mut err);
        assert!(err.is_set());
    }

    #[test]
    fn parse_border_style_rejects_a_non_string_array_item() {
        let mut items: Vec<Object> = (0..8).map(|_| Object::String(b"x".to_vec())).collect();
        items[3] = Object::Integer(7);
        let mut fc = WinConfig::default();
        let mut err = Error::default();
        parse_border_style(&Object::Array(items), &mut fc, &mut err);
        assert!(err.is_set());
    }

    #[test]
    fn parse_border_style_rejects_a_missing_corner_between_edges() {
        // Edges at 1/7 present but the 0 corner empty is rejected.
        let mut items: Vec<Object> = (0..8).map(|_| Object::String(b"x".to_vec())).collect();
        items[0] = Object::String(b"".to_vec());
        let mut fc = WinConfig::default();
        let mut err = Error::default();
        parse_border_style(&Object::Array(items), &mut fc, &mut err);
        assert!(err.is_set());
    }

    #[test]
    fn parse_border_style_ignores_an_unhandled_object_type() {
        // The original's if/else-if chain has no final else: a
        // non-String, non-Array style leaves everything but `border`
        // untouched, and reports no error.
        let mut fc = WinConfig::default();
        let mut err = Error::default();
        parse_border_style(&Object::Integer(3), &mut fc, &mut err);
        assert!(!err.is_set());
        assert!(fc.border);
        assert_eq!(slot(&fc, 0), b"");
    }

    #[test]
    fn parse_winborder_accepts_a_style_name_and_a_custom_list() {
        let mut fc = WinConfig::default();
        let mut err = Error::default();
        assert!(parse_winborder(&mut fc, b"rounded", &mut err));
        assert_eq!(slot(&fc, 0), "╭".as_bytes());

        let mut fc = WinConfig::default();
        let mut err = Error::default();
        assert!(parse_winborder(&mut fc, b"1,2,3,4,5,6,7,8", &mut err));
        assert_eq!(slot(&fc, 0), b"1");
        assert_eq!(slot(&fc, 7), b"8");
    }

    #[test]
    fn parse_winborder_rejects_a_wrong_length_custom_list() {
        // Fewer than eight parts, and more than eight parts.
        for v in [&b"1,2,3"[..], &b"1,2,3,4,5,6,7,8,9"[..]] {
            let mut fc = WinConfig::default();
            let mut err = Error::default();
            assert!(!parse_winborder(&mut fc, v, &mut err));
        }
    }

    #[test]
    fn parse_winborder_rejects_an_unknown_style_name() {
        let mut fc = WinConfig::default();
        let mut err = Error::default();
        assert!(!parse_winborder(&mut fc, b"nope", &mut err));
        assert!(err.is_set());
    }
}
