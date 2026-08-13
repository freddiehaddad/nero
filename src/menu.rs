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
//! Also translated: [`get_menu_mode_str`] (a short mode-indicator
//! string for a `menu_mode` bitmask, matching `:menu`'s own listing
//! form's letters), via already-real `crate::menu_defs::menu_mode`;
//! and [`is_menus_locked`] (whether menu changes are currently
//! disallowed), via a new `MENUS_LOCKED` depth counter mirroring the
//! original's own file-static `menus_locked` int - only ever mutated
//! by `ex_menu`'s listing form, not yet translated, so it stays `0`
//! forever today, matching `window.rs`'s `FRAME_LOCKED`'s own
//! established "untranslated mutator, provably-zero-today counter"
//! precedent. `is_menus_locked`'s own real `emsg` call when locked is
//! omitted, matching the established "skip the deferred message-
//! display side effect, keep the exact same return value" policy.
//!
//! Also translated: [`get_menu_cmd_modes`] (parses a `:menu`-family
//! command name like `"nmenu"`/`"noremenu"`/`"tlunmenu"` into its
//! `menu_mode` bit-flags plus `noremap`/`unmenu` flags) - a pure,
//! self-contained byte-parsing function with no `vimmenu_T` tree
//! dependency at all, needing only already-real
//! `crate::menu_defs::menu_mode` and `crate::input_defs::RemapValues`.
//!
//! Deferred: everything else - the whole menu tree beyond its
//! [`get_root_menu`] storage accessor, `ex_menu`/`execute_menu`/`show_menus*`/`menu_get`/
//! `menu_find`/`menu_text`/`menuitem_getinfo`, all needing the menu
//! tree/editor-command execution machinery and menu translation/remap
//! state.

use crate::menu_defs::{VimMenu, MNU_HIDDEN_CHAR};

/// Release one mode's menu command (`free_menu_string`).
#[allow(dead_code)]
fn free_menu_string(menu: &mut VimMenu, index: usize) {
    menu.strings[index] = None;
}

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

/// Returns the `\ref MENU_MODES` bit-flags specified by menu command
/// `cmd` (e.g. `:menu!` returns `MENU_CMDLINE_MODE | MENU_INSERT_MODE`),
/// along with the `noremap`/`unmenu` flags (`get_menu_cmd_modes`).
///
/// Returns `(modes, noremap, unmenu)` in place of the original's own
/// `int *noremap`/`bool *unmenu` out-parameters. `cmd`'s own advancing-
/// cursor walk (including the `default:` case's `cmd--`, which
/// resets the cursor all the way back to the very first byte) is
/// modeled as a plain `pos: usize` index into the slice, hand-traced
/// against `"nmenu"`/`"noremenu"`/`"unmenu"`/`"tlmenu"`/`"tmenu"`
/// before trusting the translation.
#[must_use]
pub fn get_menu_cmd_modes(cmd: &[u8], forceit: bool) -> (i32, crate::input_defs::RemapValues, bool) {
    use crate::input_defs::RemapValues;
    use crate::menu_defs::menu_mode::{CMDLINE, INSERT, NORMAL, OP_PENDING, SELECT, TERMINAL, TIP, VISUAL};

    let mut pos = 1usize;
    let modes = match cmd.first().copied().unwrap_or(0) {
        b'v' => VISUAL | SELECT,
        b'x' => VISUAL,
        b's' => SELECT,
        b'o' => OP_PENDING,
        b'i' => INSERT,
        b't' => {
            if cmd.get(pos).copied() == Some(b'l') {
                pos += 1;
                TERMINAL
            } else {
                TIP
            }
        }
        b'c' => CMDLINE,
        b'a' => INSERT | CMDLINE | NORMAL | VISUAL | SELECT | OP_PENDING,
        b'n' if cmd.get(pos).copied() != Some(b'o') => NORMAL,
        _ => {
            // `cmd--`: treat the whole original string as unconsumed
            // (this also covers the `'n'` case's own fallthrough,
            // e.g. "noremenu", and an empty `cmd`).
            pos = 0;
            if forceit {
                INSERT | CMDLINE
            } else {
                NORMAL | VISUAL | SELECT | OP_PENDING
            }
        }
    };

    let noremap = if cmd.get(pos).copied() == Some(b'n') {
        RemapValues::None
    } else {
        RemapValues::Yes
    };
    let unmenu = cmd.get(pos).copied() == Some(b'u');

    (modes, noremap, unmenu)
}

/// Return a short mode-indicator string for a `modes`
/// ([`crate::menu_defs::menu_mode`]) bitmask, matching the letters
/// `:menu`'s own listing form uses (`get_menu_mode_str`).
#[must_use]
pub fn get_menu_mode_str(modes: i32) -> &'static str {
    use crate::menu_defs::menu_mode::{CMDLINE, INSERT, NORMAL, OP_PENDING, SELECT, TERMINAL, TIP, VISUAL};

    if modes & (INSERT | CMDLINE | NORMAL | VISUAL | SELECT | OP_PENDING)
        == (INSERT | CMDLINE | NORMAL | VISUAL | SELECT | OP_PENDING)
    {
        return "a";
    }
    if modes & (NORMAL | VISUAL | SELECT | OP_PENDING) == (NORMAL | VISUAL | SELECT | OP_PENDING) {
        return " ";
    }
    if modes & (INSERT | CMDLINE) == (INSERT | CMDLINE) {
        return "!";
    }
    if modes & (VISUAL | SELECT) == (VISUAL | SELECT) {
        return "v";
    }
    if modes & VISUAL != 0 {
        return "x";
    }
    if modes & SELECT != 0 {
        return "s";
    }
    if modes & OP_PENDING != 0 {
        return "o";
    }
    if modes & INSERT != 0 {
        return "i";
    }
    if modes & TERMINAL != 0 {
        return "tl";
    }
    if modes & CMDLINE != 0 {
        return "c";
    }
    if modes & NORMAL != 0 {
        return "n";
    }
    if modes & TIP != 0 {
        return "t";
    }

    ""
}

/// `menus_locked` - depth counter; `> 0` means menu changes are
/// currently disallowed (e.g. while `:menu`'s own listing form is
/// executing). In the original, only ever incremented/decremented by
/// `ex_menu`'s listing form, not yet translated - so this stays `0`
/// forever in this crate today, matching the real state of any
/// session that can't yet run that form, the same "untranslated
/// mutator, provably-zero-today counter" pattern already established
/// by `window.rs`'s `FRAME_LOCKED`.
static MENUS_LOCKED: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);

/// Head of the top-level menu list (`root_menu`).
static ROOT_MENU: crate::globals::GlobalCell<*mut VimMenu> =
    crate::globals::GlobalCell::new(std::ptr::null_mut());

/// Address of the top-level menu-list pointer (`get_root_menu`).
///
/// The menu name is intentionally ignored in the original: all menu
/// paths share the same root list. Returning a raw pointer to the
/// pointer preserves callers' ability to replace the list head without
/// handing out a long-lived mutable reference into global storage.
#[must_use]
pub fn get_root_menu(_name: &[u8]) -> *mut *mut VimMenu {
    ROOT_MENU.as_ptr()
}

/// Whether menu changes are currently locked (`is_menus_locked`). The
/// original's own `emsg` call when locked is omitted, matching this
/// crate's established "skip the deferred message-display side
/// effect, keep the exact same return value" policy.
#[must_use]
pub fn is_menus_locked() -> bool {
    // SAFETY: a plain read through one exclusive borrow.
    unsafe { *MENUS_LOCKED.get_mut() > 0 }
}

/// Removes backslash escapes from the first part of a menu name
/// (`menu_unescape_name`).
///
/// Only the leading part is unescaped: the scan stops at the first
/// `.`, which separates menu-name parts. An escaped `.` does not
/// terminate the scan, because the character after a backslash is
/// stepped over whole - so `"a\\.b"` unescapes to `"a.b"` rather than
/// stopping at the dot.
///
/// The original rewrites the name in place with `STRMOVE`; this
/// rebuilds it, which is the same result without the overlapping
/// copy.
pub fn menu_unescape_name(name: &mut Vec<u8>) {
    let mut out: Vec<u8> = Vec::with_capacity(name.len());
    let mut i = 0;

    while i < name.len() && name[i] != b'.' {
        if name[i] == b'\\' {
            // Drop the backslash and take what follows verbatim, so an
            // escaped '.' is kept rather than ending the scan.
            i += 1;
            if i >= name.len() {
                break;
            }
        }
        let len = (crate::mbyte::utf_ptr2len(&name[i..]) as usize).max(1);
        let end = (i + len).min(name.len());
        out.extend_from_slice(&name[i..end]);
        i = end;
    }

    // Everything from the separator on is left exactly as it was.
    out.extend_from_slice(&name[i..]);
    *name = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_menu_string_clears_only_the_selected_mode() {
        let mut menu = VimMenu::default();
        menu.strings[0] = Some(b"shared-command".to_vec());
        menu.strings[1] = Some(b"shared-command".to_vec());
        free_menu_string(&mut menu, 0);
        assert!(menu.strings[0].is_none());
        assert_eq!(
            menu.strings[1].as_deref(),
            Some(b"shared-command".as_slice())
        );
    }

    struct RootMenuGuard(*mut VimMenu);

    impl RootMenuGuard {
        fn save() -> Self {
            Self(unsafe { *ROOT_MENU.get_mut() })
        }
    }

    impl Drop for RootMenuGuard {
        fn drop(&mut self) {
            unsafe { *ROOT_MENU.get_mut() = self.0 };
        }
    }

    #[test]
    fn get_root_menu_returns_the_same_storage_for_every_path() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = RootMenuGuard::save();
        unsafe { *ROOT_MENU.get_mut() = std::ptr::null_mut() };

        let file = get_root_menu(b"File.New");
        let edit = get_root_menu(b"Edit.Undo");
        assert_eq!(file, edit);
        assert!(unsafe { *file }.is_null());
    }

    #[test]
    fn get_root_menu_allows_replacing_the_root_head() {
        let _lock = crate::globals::global_state_test_lock();
        let mut menu = Box::new(VimMenu::default());
        let menu_ptr = std::ptr::addr_of_mut!(*menu);
        let _g = RootMenuGuard::save();

        unsafe { *get_root_menu(b"anything") = menu_ptr };

        assert_eq!(unsafe { *get_root_menu(b"") }, menu_ptr);
    }

    // --- menu_unescape_name ---

    fn unescaped(s: &[u8]) -> Vec<u8> {
        let mut v = s.to_vec();
        menu_unescape_name(&mut v);
        v
    }

    #[test]
    fn menu_unescape_name_removes_backslashes() {
        assert_eq!(unescaped(br"a\ b"), b"a b".to_vec());
        assert_eq!(unescaped(b"plain"), b"plain".to_vec());
    }

    /// An escaped dot is kept as a literal and does NOT end the scan,
    /// because the character after a backslash is stepped over whole.
    #[test]
    fn menu_unescape_name_keeps_an_escaped_dot() {
        assert_eq!(unescaped(br"a\.b"), b"a.b".to_vec());
    }

    /// An unescaped dot separates menu parts, so everything from it
    /// on is left untouched - including later backslashes.
    #[test]
    fn menu_unescape_name_stops_at_an_unescaped_dot() {
        assert_eq!(unescaped(br"a\ b.c\ d"), br"a b.c\ d".to_vec());
    }

    #[test]
    fn menu_unescape_name_handles_a_trailing_backslash() {
        assert_eq!(unescaped(br"ab\"), b"ab".to_vec());
    }

    #[test]
    fn menu_unescape_name_handles_an_empty_name() {
        assert_eq!(unescaped(b""), b"".to_vec());
    }

    /// Multi-byte characters are stepped over whole, so their
    /// continuation bytes are never mistaken for a backslash or dot.
    #[test]
    fn menu_unescape_name_preserves_multibyte_characters() {
        assert_eq!(unescaped("héllo".as_bytes()), "héllo".as_bytes().to_vec());
        assert_eq!(unescaped(r"h\éllo".as_bytes()), "héllo".as_bytes().to_vec());
    }

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

    // ---- get_menu_cmd_modes ----

    #[test]
    fn get_menu_cmd_modes_nmenu_is_normal_remap_yes() {
        use crate::input_defs::RemapValues;
        use crate::menu_defs::menu_mode::NORMAL;
        assert_eq!(get_menu_cmd_modes(b"nmenu", false), (NORMAL, RemapValues::Yes, false));
    }

    #[test]
    fn get_menu_cmd_modes_noremenu_falls_through_to_default_with_remap_none() {
        use crate::input_defs::RemapValues;
        use crate::menu_defs::menu_mode::{NORMAL, OP_PENDING, SELECT, VISUAL};
        // "noremenu" - the 'n' case's own `*cmd != 'o'` guard fails
        // (the very next byte IS 'o'), falling through to `default`,
        // which resets the cursor all the way back to the start - so
        // `noremap` is then decided by the FIRST byte ('n') again.
        assert_eq!(
            get_menu_cmd_modes(b"noremenu", false),
            (NORMAL | VISUAL | SELECT | OP_PENDING, RemapValues::None, false)
        );
    }

    #[test]
    fn get_menu_cmd_modes_unmenu_is_unmenu_true() {
        use crate::input_defs::RemapValues;
        use crate::menu_defs::menu_mode::{NORMAL, OP_PENDING, SELECT, VISUAL};
        assert_eq!(
            get_menu_cmd_modes(b"unmenu", false),
            (NORMAL | VISUAL | SELECT | OP_PENDING, RemapValues::Yes, true)
        );
    }

    #[test]
    fn get_menu_cmd_modes_tlmenu_is_terminal() {
        use crate::input_defs::RemapValues;
        use crate::menu_defs::menu_mode::TERMINAL;
        assert_eq!(get_menu_cmd_modes(b"tlmenu", false), (TERMINAL, RemapValues::Yes, false));
    }

    #[test]
    fn get_menu_cmd_modes_tmenu_is_tip() {
        use crate::input_defs::RemapValues;
        use crate::menu_defs::menu_mode::TIP;
        assert_eq!(get_menu_cmd_modes(b"tmenu", false), (TIP, RemapValues::Yes, false));
    }

    #[test]
    fn get_menu_cmd_modes_vmenu_is_visual_and_select() {
        use crate::input_defs::RemapValues;
        use crate::menu_defs::menu_mode::{SELECT, VISUAL};
        assert_eq!(get_menu_cmd_modes(b"vmenu", false), (VISUAL | SELECT, RemapValues::Yes, false));
    }

    #[test]
    fn get_menu_cmd_modes_amenu_is_all_six_modes() {
        use crate::input_defs::RemapValues;
        use crate::menu_defs::menu_mode::{CMDLINE, INSERT, NORMAL, OP_PENDING, SELECT, VISUAL};
        assert_eq!(
            get_menu_cmd_modes(b"amenu", false),
            (INSERT | CMDLINE | NORMAL | VISUAL | SELECT | OP_PENDING, RemapValues::Yes, false)
        );
    }

    #[test]
    fn get_menu_cmd_modes_bare_menu_without_forceit() {
        use crate::input_defs::RemapValues;
        use crate::menu_defs::menu_mode::{NORMAL, OP_PENDING, SELECT, VISUAL};
        // "menu" - 'm' matches none of the explicit cases, falling
        // straight to `default` (no fallthrough needed this time).
        assert_eq!(
            get_menu_cmd_modes(b"menu", false),
            (NORMAL | VISUAL | SELECT | OP_PENDING, RemapValues::Yes, false)
        );
    }

    #[test]
    fn get_menu_cmd_modes_bare_menu_bang_with_forceit() {
        use crate::input_defs::RemapValues;
        use crate::menu_defs::menu_mode::{CMDLINE, INSERT};
        // "menu!!" (forceit=true) - the original's own ":menu!!" form.
        assert_eq!(
            get_menu_cmd_modes(b"menu!!", true),
            (INSERT | CMDLINE, RemapValues::Yes, false)
        );
    }

    #[test]
    fn get_menu_cmd_modes_empty_string_behaves_like_default() {
        use crate::input_defs::RemapValues;
        use crate::menu_defs::menu_mode::{NORMAL, OP_PENDING, SELECT, VISUAL};
        assert_eq!(
            get_menu_cmd_modes(b"", false),
            (NORMAL | VISUAL | SELECT | OP_PENDING, RemapValues::Yes, false)
        );
    }

    // ---- get_menu_mode_str ----

    use crate::menu_defs::menu_mode;

    #[test]
    fn get_menu_mode_str_all_six_main_modes_is_a() {
        let modes = menu_mode::INSERT
            | menu_mode::CMDLINE
            | menu_mode::NORMAL
            | menu_mode::VISUAL
            | menu_mode::SELECT
            | menu_mode::OP_PENDING;
        assert_eq!(get_menu_mode_str(modes), "a");
    }

    #[test]
    fn get_menu_mode_str_normal_visual_select_op_pending_is_space() {
        let modes = menu_mode::NORMAL | menu_mode::VISUAL | menu_mode::SELECT | menu_mode::OP_PENDING;
        assert_eq!(get_menu_mode_str(modes), " ");
    }

    #[test]
    fn get_menu_mode_str_insert_and_cmdline_is_bang() {
        assert_eq!(get_menu_mode_str(menu_mode::INSERT | menu_mode::CMDLINE), "!");
    }

    #[test]
    fn get_menu_mode_str_visual_and_select_is_v() {
        assert_eq!(get_menu_mode_str(menu_mode::VISUAL | menu_mode::SELECT), "v");
    }

    #[test]
    fn get_menu_mode_str_visual_only_is_x() {
        assert_eq!(get_menu_mode_str(menu_mode::VISUAL), "x");
    }

    #[test]
    fn get_menu_mode_str_select_only_is_s() {
        assert_eq!(get_menu_mode_str(menu_mode::SELECT), "s");
    }

    #[test]
    fn get_menu_mode_str_op_pending_only_is_o() {
        assert_eq!(get_menu_mode_str(menu_mode::OP_PENDING), "o");
    }

    #[test]
    fn get_menu_mode_str_insert_only_is_i() {
        assert_eq!(get_menu_mode_str(menu_mode::INSERT), "i");
    }

    #[test]
    fn get_menu_mode_str_terminal_only_is_tl() {
        assert_eq!(get_menu_mode_str(menu_mode::TERMINAL), "tl");
    }

    #[test]
    fn get_menu_mode_str_cmdline_only_is_c() {
        assert_eq!(get_menu_mode_str(menu_mode::CMDLINE), "c");
    }

    #[test]
    fn get_menu_mode_str_normal_only_is_n() {
        assert_eq!(get_menu_mode_str(menu_mode::NORMAL), "n");
    }

    #[test]
    fn get_menu_mode_str_tip_only_is_t() {
        assert_eq!(get_menu_mode_str(menu_mode::TIP), "t");
    }

    #[test]
    fn get_menu_mode_str_zero_is_empty() {
        assert_eq!(get_menu_mode_str(0), "");
    }

    // ---- is_menus_locked ----

    #[test]
    fn is_menus_locked_is_false_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(!is_menus_locked());
    }
}
