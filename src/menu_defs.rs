//! Translated from `src/nvim/menu_defs.h`.

/// Indices into [`VimMenu`]'s `strings`/`noremap`/`silent` arrays for each
/// mode (`MENU_INDEX_*`).
pub mod menu_index {
    pub const INVALID: i32 = -1;
    pub const NORMAL: i32 = 0;
    pub const VISUAL: i32 = 1;
    pub const SELECT: i32 = 2;
    pub const OP_PENDING: i32 = 3;
    pub const INSERT: i32 = 4;
    pub const CMDLINE: i32 = 5;
    pub const TERMINAL: i32 = 6;
    pub const TIP: i32 = 7;
    /// note `TIP` is not a 'real' mode
    pub const MODES: usize = 8;
}

/// Menu modes (`MENU_*_MODE`, bit flags derived from [`menu_index`]).
pub mod menu_mode {
    use super::menu_index;

    pub const NORMAL: i32 = 1 << menu_index::NORMAL;
    pub const VISUAL: i32 = 1 << menu_index::VISUAL;
    pub const SELECT: i32 = 1 << menu_index::SELECT;
    pub const OP_PENDING: i32 = 1 << menu_index::OP_PENDING;
    pub const INSERT: i32 = 1 << menu_index::INSERT;
    pub const CMDLINE: i32 = 1 << menu_index::CMDLINE;
    pub const TERMINAL: i32 = 1 << menu_index::TERMINAL;
    pub const TIP: i32 = 1 << menu_index::TIP;
    pub const ALL_MODES: i32 = (1 << menu_index::TIP) - 1;
}

/// Start a menu name with this to not include it on the main menu bar
/// (`MNU_HIDDEN_CHAR`).
pub const MNU_HIDDEN_CHAR: u8 = b']';

/// A menu tree node (`struct VimMenu`, typedef'd as `vimmenu_T`).
///
/// `children`/`parent`/`next` stay raw pointers (an intrusive tree/list,
/// matching this crate's established convention for such structures
/// elsewhere, e.g. `MtNode`/`FrameT`).
pub struct VimMenu {
    /// Which modes is this menu visible for
    pub modes: i32,
    /// for which modes the menu is enabled
    pub enabled: i32,
    /// Name of menu, possibly translated
    pub name: Option<Vec<u8>>,
    /// Displayed Name ("name" without '&')
    pub dname: Option<Vec<u8>>,
    /// "name" untranslated, `None` when was not translated
    pub en_name: Option<Vec<u8>>,
    /// `None` when "dname" untranslated
    pub en_dname: Option<Vec<u8>>,
    /// mnemonic key (after '&')
    pub mnemonic: i32,
    /// accelerator text (after TAB)
    pub actext: Option<Vec<u8>>,
    /// Menu order priority
    pub priority: i32,
    /// Mapped string for each mode
    pub strings: [Option<Vec<u8>>; menu_index::MODES],
    /// A `REMAP_*` flag for each mode
    pub noremap: [i32; menu_index::MODES],
    /// A silent flag for each mode
    pub silent: [bool; menu_index::MODES],
    /// Children of sub-menu
    pub children: *mut VimMenu,
    /// Parent of menu
    pub parent: *mut VimMenu,
    /// Next item in menu
    pub next: *mut VimMenu,
}

impl Default for VimMenu {
    fn default() -> Self {
        VimMenu {
            modes: 0,
            enabled: 0,
            name: None,
            dname: None,
            en_name: None,
            en_dname: None,
            mnemonic: 0,
            actext: None,
            priority: 0,
            strings: std::array::from_fn(|_| None),
            noremap: [0; menu_index::MODES],
            silent: [false; menu_index::MODES],
            children: std::ptr::null_mut(),
            parent: std::ptr::null_mut(),
            next: std::ptr::null_mut(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_mode_flags_match_indices() {
        assert_eq!(menu_mode::NORMAL, 1);
        assert_eq!(menu_mode::VISUAL, 2);
        assert_eq!(menu_mode::TIP, 1 << 7);
        assert_eq!(menu_mode::ALL_MODES, (1 << 7) - 1);
    }

    #[test]
    fn vim_menu_default_has_null_links_and_no_strings() {
        let m = VimMenu::default();
        assert!(m.children.is_null());
        assert!(m.parent.is_null());
        assert!(m.next.is_null());
        assert!(m.strings.iter().all(|s| s.is_none()));
        assert!(m.silent.iter().all(|&s| !s));
        assert_eq!(m.strings.len(), menu_index::MODES);
    }
}
