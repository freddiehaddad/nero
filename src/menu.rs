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
//! [`menu_name_equal`] (and its own helper, `menu_namecmp`).
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
}
