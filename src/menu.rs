//! Translated from `src/nvim/menu.c` (tractable core only).
//!
//! `menu.c` (~2600 lines) manages neovim's menu tree (`:menu`, GUI/TUI
//! menu bar and popup menus) - almost entirely dependent on the not-yet-
//! translated menu tree construction/execution machinery.
//!
//! Translated: a handful of small, pure predicates/comparisons over menu
//! *names*, needing only [`crate::menu_defs`]'s already-translated
//! struct shapes - [`menu_is_menubar`], [`menu_is_popup`],
//! [`menu_is_toolbar`], [`menu_is_winbar`], [`menu_is_separator`],
//! [`menu_name_equal`] (and its own helper, `menu_namecmp`),
//! [`menu_is_hidden`]; and the current-mode resolvers `get_menu_mode`
//! (private, matching the original's own `static`) and
//! [`get_menu_mode_flag`], needing only already-real `GLOBALS.State`/
//! `Visual.active`/`Visual.select`/`finish_op` plus
//! `crate::menu_defs::menu_index`/`menu_mode` (both already
//! translated ahead of this file, from `menu_defs.h`).
//!
//! Also translated: [`menu_skip_part`] (skip one dot-separated menu-
//! name part, honoring backslash/`Ctrl-V`-escaped characters), via
//! already-real `crate::ascii_defs::ascii_iswhite`/`CTRL_V`. Modeled
//! on a plain `&[u8]` starting at the position of interest, returning
//! the number of bytes consumed, matching this crate's established
//! "operate on indices into a byte slice, not raw pointers" idiom for
//! string-scanning functions (e.g. `charset.rs`'s `getdigits_int`).
//! Translated ahead of its real callers (`ex_menu`/`menu_namecmp`'s
//! own callers in the not-yet-translated menu-command parser),
//! matching the established "translate ahead of a real caller"
//! precedent.
//!
//! Deferred: everything else - the whole menu tree (`root_menu`/
//! `get_root_menu`), `ex_menu`/`execute_menu`/`show_menus*`/`menu_get`/
//! `menu_find`/`get_menu_cmd_modes`/`menu_text`/`menuitem_getinfo`, all
//! needing the menu tree/editor-command execution machinery and menu
//! translation/remap state.

use crate::menu_defs::{VimMenu, MNU_HIDDEN_CHAR};

/// Whether `name` is a popup menu name (`menu_is_popup`).
#[must_use]
pub fn menu_is_popup(name: &[u8]) -> bool {
    name.starts_with(b"PopUp")
}

/// Whether `name` is a toolbar menu name (`menu_is_toolbar`).
#[must_use]
pub fn menu_is_toolbar(name: &[u8]) -> bool {
    name.starts_with(b"ToolBar")
}

/// Whether `name` is a window toolbar menu name (`menu_is_winbar`).
#[must_use]
pub fn menu_is_winbar(name: &[u8]) -> bool {
    name.starts_with(b"WinBar")
}

/// Whether `name` can be a menu in the MenuBar (`menu_is_menubar`).
#[must_use]
pub fn menu_is_menubar(name: &[u8]) -> bool {
    !menu_is_popup(name)
        && !menu_is_toolbar(name)
        && !menu_is_winbar(name)
        && name.first() != Some(&MNU_HIDDEN_CHAR)
}

/// Whether `name` is a menu separator identifier: starts AND ends with
/// `-` (`menu_is_separator`).
///
/// The original's own `name[strlen(name) - 1]` would underflow for an
/// empty `name`, but is never actually reached in that case: the first
/// condition (`name[0] == '-'`, `NUL` for an empty string) is already
/// false, and C's `&&` short-circuits - reproduced here the same way,
/// since Rust's `&&` short-circuits identically and `name.first()` is
/// `None` (not `Some(&b'-')`) for an empty slice.
#[must_use]
pub fn menu_is_separator(name: &[u8]) -> bool {
    name.first() == Some(&b'-') && name.last() == Some(&b'-')
}

/// Whether `name` matches `mname`, comparing only up to the first NUL
/// (the end of the slice) or TAB in either string - text after a TAB is
/// an accelerator/description suffix, ignored for comparison purposes
/// (`menu_namecmp`).
///
/// A `None` `mname` (a menu name field that was never set) never
/// matches - this crate's own safe interpretation of a comparison the
/// original could not even attempt in that scenario without risking a
/// NULL dereference.
fn menu_namecmp(name: &[u8], mname: Option<&[u8]>) -> bool {
    let Some(mname) = mname else { return false };
    let mut i = 0;
    while i < name.len() && name[i] != crate::ascii_defs::TAB {
        if i >= mname.len() || name[i] != mname[i] {
            break;
        }
        i += 1;
    }
    let name_end = i >= name.len() || name[i] == crate::ascii_defs::TAB;
    let mname_end = i >= mname.len() || mname[i] == crate::ascii_defs::TAB;
    name_end && mname_end
}

/// Whether `name` matches `menu` - compared two ways (the raw menu name
/// and the menu name without `&`), ignoring anything after a TAB, and
/// checked against both the translated and untranslated (`en_`-
/// prefixed) names when the menu has a translation (`menu_name_equal`).
#[must_use]
pub fn menu_name_equal(name: &[u8], menu: &VimMenu) -> bool {
    if menu.en_name.is_some()
        && (menu_namecmp(name, menu.en_name.as_deref())
            || menu_namecmp(name, menu.en_dname.as_deref()))
    {
        return true;
    }
    menu_namecmp(name, menu.name.as_deref()) || menu_namecmp(name, menu.dname.as_deref())
}

/// Whether `name` is a hidden menu name: starts with [`MNU_HIDDEN_CHAR`]
/// (`']'`), or is a `PopUp` menu whose 6th byte is present (i.e. has
/// something following the plain `"PopUp"` prefix beyond a bare mode
/// suffix - matching the original's own `name[5] != NUL` check exactly)
/// (`menu_is_hidden`).
#[must_use]
pub fn menu_is_hidden(name: &[u8]) -> bool {
    name.first() == Some(&MNU_HIDDEN_CHAR) || (menu_is_popup(name) && name.get(5).is_some())
}

/// Resolve the current mode into a [`crate::menu_defs::menu_index`]
/// value, or `INVALID` if none of the recognized modes apply
/// (`get_menu_mode`).
///
/// # Safety
/// `crate::globals::GLOBALS` must be a valid, initialized singleton
/// (same requirement as every other function reading it).
#[must_use]
unsafe fn get_menu_mode() -> i32 {
    use crate::menu_defs::menu_index;
    use crate::state_defs::mode;

    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };

    if g.State & mode::TERMINAL as i32 != 0 {
        return menu_index::TERMINAL;
    }
    if g.Visual.active {
        if g.Visual.select {
            return menu_index::SELECT;
        }
        return menu_index::VISUAL;
    }
    if g.State & mode::INSERT as i32 != 0 {
        return menu_index::INSERT;
    }
    if g.State & mode::CMDLINE as i32 != 0
        || g.State == mode::ASKMORE as i32
        || g.State == mode::HITRETURN as i32
    {
        return menu_index::CMDLINE;
    }
    if g.finish_op {
        return menu_index::OP_PENDING;
    }
    if g.State & mode::NORMAL as i32 != 0 {
        return menu_index::NORMAL;
    }
    if g.State & mode::LANGMAP as i32 != 0 {
        // must be a "r" command, like Insert mode
        return menu_index::INSERT;
    }
    menu_index::INVALID
}

/// The bit-flag ([`crate::menu_defs::menu_mode`]) form of
/// `get_menu_mode()` - `0` when no recognized mode applies
/// (`get_menu_mode_flag`).
///
/// # Safety
/// Same as `get_menu_mode()`.
#[must_use]
pub unsafe fn get_menu_mode_flag() -> i32 {
    use crate::menu_defs::menu_index;

    // SAFETY: forwarded from this function's own safety doc.
    let mode = unsafe { get_menu_mode() };
    if mode == menu_index::INVALID {
        return 0;
    }
    1 << mode
}

/// Skip one dot-separated menu-name part of `s`, honoring backslash/
/// `Ctrl-V`-escaped characters (skipped over rather than treated as a
/// terminator), and return the number of bytes making up that part
/// (up to, but not including, the first unescaped `.`/whitespace, or
/// the end of `s` if neither ever occurs) (`menu_skip_part`).
#[must_use]
pub fn menu_skip_part(s: &[u8]) -> usize {
    let mut p = 0;
    while p < s.len() && s[p] != b'.' && !crate::ascii_defs::ascii_iswhite(i32::from(s[p])) {
        if (s[p] == b'\\' || s[p] == crate::ascii_defs::CTRL_V) && p + 1 < s.len() {
            p += 1;
        }
        p += 1;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_is_popup_matches_prefix() {
        assert!(menu_is_popup(b"PopUp"));
        assert!(menu_is_popup(b"PopUpFile"));
        assert!(!menu_is_popup(b"File"));
        assert!(!menu_is_popup(b""));
    }

    #[test]
    fn menu_is_toolbar_matches_prefix() {
        assert!(menu_is_toolbar(b"ToolBar"));
        assert!(menu_is_toolbar(b"ToolBar.Open"));
        assert!(!menu_is_toolbar(b"PopUp"));
    }

    #[test]
    fn menu_is_winbar_matches_prefix() {
        assert!(menu_is_winbar(b"WinBar"));
        assert!(!menu_is_winbar(b"ToolBar"));
    }

    #[test]
    fn menu_is_menubar_excludes_special_menus() {
        assert!(menu_is_menubar(b"File"));
        assert!(!menu_is_menubar(b"PopUp"));
        assert!(!menu_is_menubar(b"ToolBar"));
        assert!(!menu_is_menubar(b"WinBar"));
        assert!(!menu_is_menubar(b"]Hidden"));
    }

    #[test]
    fn menu_is_separator_requires_both_ends() {
        assert!(menu_is_separator(b"-"));
        assert!(menu_is_separator(b"-SEP1-"));
        assert!(!menu_is_separator(b"-notasep"));
        assert!(!menu_is_separator(b"notasep-"));
        assert!(!menu_is_separator(b""));
    }

    #[test]
    fn menu_namecmp_exact_match() {
        assert!(menu_namecmp(b"File", Some(b"File")));
        assert!(!menu_namecmp(b"File", Some(b"Files")));
        assert!(!menu_namecmp(b"Files", Some(b"File")));
        assert!(!menu_namecmp(b"File", Some(b"Edit")));
    }

    #[test]
    fn menu_namecmp_ignores_tab_suffix() {
        // A menu's own "name\taccelerator" form should match the plain
        // "name" a caller searches for.
        assert!(menu_namecmp(b"File\tCtrl-F", Some(b"File")));
        assert!(menu_namecmp(b"File", Some(b"File\tCtrl-F")));
    }

    #[test]
    fn menu_namecmp_none_never_matches() {
        assert!(!menu_namecmp(b"File", None));
        assert!(!menu_namecmp(b"", None));
    }

    #[test]
    fn menu_name_equal_checks_name_and_dname() {
        let menu = VimMenu {
            name: Some(b"File".to_vec()),
            dname: Some(b"&File".to_vec()),
            ..Default::default()
        };
        assert!(menu_name_equal(b"File", &menu));
        assert!(menu_name_equal(b"&File", &menu));
        assert!(!menu_name_equal(b"Edit", &menu));
    }

    #[test]
    fn menu_name_equal_prefers_translated_names_when_present() {
        let menu = VimMenu {
            name: Some("Fichier".as_bytes().to_vec()),
            dname: Some("Fichier".as_bytes().to_vec()),
            en_name: Some(b"File".to_vec()),
            en_dname: Some(b"File".to_vec()),
            ..Default::default()
        };
        // Both the translated and the original English name should match.
        assert!(menu_name_equal(b"File", &menu));
        assert!(menu_name_equal("Fichier".as_bytes(), &menu));
        assert!(!menu_name_equal(b"Edit", &menu));
    }

    #[test]
    fn menu_is_hidden_true_for_leading_hidden_char() {
        assert!(menu_is_hidden(b"]File"));
    }

    #[test]
    fn menu_is_hidden_true_for_a_popup_with_a_mode_suffix() {
        // "PopUp" is exactly 5 bytes; a 6th byte present (e.g. the "i"
        // in "PopUpi") means name[5] != NUL in the original.
        assert!(menu_is_hidden(b"PopUpi"));
    }

    #[test]
    fn menu_is_hidden_false_for_a_bare_popup_name() {
        // "PopUp" alone has no 6th byte - name[5] == NUL originally.
        assert!(!menu_is_hidden(b"PopUp"));
    }

    #[test]
    fn menu_is_hidden_false_for_an_ordinary_name() {
        assert!(!menu_is_hidden(b"File"));
        assert!(!menu_is_hidden(b""));
    }

    /// RAII guard restoring the `GLOBALS` fields `get_menu_mode`/
    /// `get_menu_mode_flag` read, on drop (even on test panic).
    struct MenuModeGuard {
        prev_state: i32,
        prev_visual_active: bool,
        prev_visual_select: bool,
        prev_finish_op: bool,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl MenuModeGuard {
        fn set(state: i32, visual_active: bool, visual_select: bool, finish_op: bool) -> Self {
            let _lock = crate::globals::global_state_test_lock();
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = Self {
                prev_state: g.State,
                prev_visual_active: g.Visual.active,
                prev_visual_select: g.Visual.select,
                prev_finish_op: g.finish_op,
                _lock,
            };
            g.State = state;
            g.Visual.active = visual_active;
            g.Visual.select = visual_select;
            g.finish_op = finish_op;
            guard
        }
    }

    impl Drop for MenuModeGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            g.State = self.prev_state;
            g.Visual.active = self.prev_visual_active;
            g.Visual.select = self.prev_visual_select;
            g.finish_op = self.prev_finish_op;
        }
    }

    #[test]
    fn get_menu_mode_flag_terminal() {
        let _guard = MenuModeGuard::set(crate::state_defs::mode::TERMINAL as i32, false, false, false);
        assert_eq!(unsafe { get_menu_mode_flag() }, crate::menu_defs::menu_mode::TERMINAL);
    }

    #[test]
    fn get_menu_mode_flag_visual_and_select() {
        {
            let _guard = MenuModeGuard::set(crate::state_defs::mode::NORMAL as i32, true, false, false);
            assert_eq!(unsafe { get_menu_mode_flag() }, crate::menu_defs::menu_mode::VISUAL);
        }
        {
            let _guard = MenuModeGuard::set(crate::state_defs::mode::NORMAL as i32, true, true, false);
            assert_eq!(unsafe { get_menu_mode_flag() }, crate::menu_defs::menu_mode::SELECT);
        }
    }

    #[test]
    fn get_menu_mode_flag_insert() {
        let _guard = MenuModeGuard::set(crate::state_defs::mode::INSERT as i32, false, false, false);
        assert_eq!(unsafe { get_menu_mode_flag() }, crate::menu_defs::menu_mode::INSERT);
    }

    #[test]
    fn get_menu_mode_flag_cmdline_via_bit_and_via_exact_states() {
        {
            let _guard = MenuModeGuard::set(crate::state_defs::mode::CMDLINE as i32, false, false, false);
            assert_eq!(unsafe { get_menu_mode_flag() }, crate::menu_defs::menu_mode::CMDLINE);
        }
        {
            let _guard = MenuModeGuard::set(crate::state_defs::mode::ASKMORE as i32, false, false, false);
            assert_eq!(unsafe { get_menu_mode_flag() }, crate::menu_defs::menu_mode::CMDLINE);
        }
        {
            let _guard = MenuModeGuard::set(crate::state_defs::mode::HITRETURN as i32, false, false, false);
            assert_eq!(unsafe { get_menu_mode_flag() }, crate::menu_defs::menu_mode::CMDLINE);
        }
    }

    #[test]
    fn get_menu_mode_flag_op_pending() {
        let _guard = MenuModeGuard::set(0, false, false, true);
        assert_eq!(unsafe { get_menu_mode_flag() }, crate::menu_defs::menu_mode::OP_PENDING);
    }

    #[test]
    fn get_menu_mode_flag_normal() {
        let _guard = MenuModeGuard::set(crate::state_defs::mode::NORMAL as i32, false, false, false);
        assert_eq!(unsafe { get_menu_mode_flag() }, crate::menu_defs::menu_mode::NORMAL);
    }

    #[test]
    fn get_menu_mode_flag_langmap_reports_insert() {
        let _guard = MenuModeGuard::set(crate::state_defs::mode::LANGMAP as i32, false, false, false);
        assert_eq!(unsafe { get_menu_mode_flag() }, crate::menu_defs::menu_mode::INSERT);
    }

    #[test]
    fn get_menu_mode_flag_invalid_is_zero() {
        let _guard = MenuModeGuard::set(0, false, false, false);
        assert_eq!(unsafe { get_menu_mode_flag() }, 0);
    }

    #[test]
    fn get_menu_mode_flag_terminal_takes_priority_over_visual() {
        // TERMINAL is checked FIRST in the original, even if Visual
        // happens to also be active.
        let _guard = MenuModeGuard::set(crate::state_defs::mode::TERMINAL as i32, true, false, false);
        assert_eq!(unsafe { get_menu_mode_flag() }, crate::menu_defs::menu_mode::TERMINAL);
    }

    // ---- menu_skip_part ----

    #[test]
    fn menu_skip_part_stops_at_a_dot() {
        assert_eq!(menu_skip_part(b"File.Edit"), 4);
    }

    #[test]
    fn menu_skip_part_stops_at_whitespace() {
        assert_eq!(menu_skip_part(b"File Edit"), 4);
    }

    #[test]
    fn menu_skip_part_consumes_the_whole_slice_when_no_terminator() {
        assert_eq!(menu_skip_part(b"File"), 4);
    }

    #[test]
    fn menu_skip_part_skips_a_backslash_escaped_dot() {
        // "File\.Edit.More" - the escaped dot at index 5 is NOT a
        // terminator, so the part continues through "Edit" and stops
        // at the SECOND (unescaped) dot, index 10.
        assert_eq!(menu_skip_part(b"File\\.Edit.More"), 10);
    }

    #[test]
    fn menu_skip_part_skips_a_ctrl_v_escaped_whitespace() {
        // "File\x16 Edit More" - Ctrl-V escapes the space at index 5,
        // so the part continues through "Edit" and stops at the next
        // (unescaped) whitespace, index 10.
        let mut s = b"File".to_vec();
        s.push(crate::ascii_defs::CTRL_V);
        s.extend_from_slice(b" Edit More");
        assert_eq!(menu_skip_part(&s), 10);
    }

    #[test]
    fn menu_skip_part_lone_trailing_backslash_is_not_an_escape() {
        // A backslash as the very LAST byte has no next byte to
        // escape, so it's just consumed like any other character.
        assert_eq!(menu_skip_part(b"File\\"), 5);
    }
}
