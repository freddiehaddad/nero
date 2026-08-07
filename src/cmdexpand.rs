//! Translated from `src/nvim/cmdexpand.c` (tractable core only).
//!
//! `cmdexpand.c` (~4000 lines) implements neovim's command-line
//! completion (`<Tab>` completion, wildmenu) - almost entirely
//! dependent on the completion-context/expansion machinery
//! (`ExpandOne`/ `nextwild`/the `expand_T` state machine), none of
//! which is translated.
//!
//! Translated: [`cmdline_fuzzy_complete`] and [`sort_func_compare`] -
//! both pure functions needing only already-translated option fields
//! (`option_vars.rs`) or plain byte-string comparison.
//!
//! Also translated: [`cmdline_compl_use_pum`] plus the three
//! self-contained `ExpandGeneric()` argument callbacks
//! [`get_retab_arg`]/[`get_messages_arg`]/[`get_mapclear_arg`], whose
//! values were read out of a real `nvim` via
//! `getcompletion('retab ', 'cmdline')` and friends. The remaining
//! `get_*_arg` callbacks in that group stay deferred: they run Lua
//! (`get_arg1_from_lua`/`nlua_exec`).
//!
//! Deferred: everything else - `nextwild`/`copy_substring_from_pos`/
//! `is_regex_match`/`concat_pattern_with_buffer_match`/
//! `expand_pattern_in_buf` (the completion/search machinery),
//! `wildescape`/`ExpandEscape` (need `vim_strsave_fnameescape`/
//! `escape_fname`/`tilde_replace`, not translated).

/// Whether the popup menu should be used for cmdline completion
/// wildmenu (`cmdline_compl_use_pum`).
///
/// `need_wildmenu` is whether the current `'wildmode'` part wants a
/// wildmenu at all.
///
/// The first branch is deliberately narrow: `'wildoptions'` having
/// `pum` is not enough on its own, because an external cmdline UI
/// without its own cmdline window draws the menu itself. The two
/// remaining branches are unconditional UI capabilities.
///
/// # Safety
/// Must not run concurrently with any write to
/// `crate::option_vars::OPTION_VARS` or `crate::globals::GLOBALS`.
#[must_use]
pub unsafe fn cmdline_compl_use_pum(need_wildmenu: bool) -> bool {
    use crate::ui::{ui_has, UiExtension};

    // SAFETY: forwarded from this function's own safety doc.
    let wop_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.wop_flags;
    // SAFETY: forwarded from this function's own safety doc.
    let cmdline_win = unsafe { crate::globals::GLOBALS.get_mut() }.cmdline_win;

    let external_cmdline_draws_it = ui_has(UiExtension::Cmdline) && cmdline_win.is_null();
    if need_wildmenu
        && wop_flags & crate::option_vars::opt_wop_flag::PUM != 0
        && !external_cmdline_draws_it
    {
        return true;
    }
    ui_has(UiExtension::Wildmenu)
        || (ui_has(UiExtension::Cmdline) && ui_has(UiExtension::Popupmenu))
}

/// The possible arguments of `:retab {-indentonly}` (`get_retab_arg`).
///
/// Returns `None` past the end, which is how `ExpandGeneric()` learns
/// to stop.
#[must_use]
pub fn get_retab_arg(idx: i32) -> Option<&'static str> {
    (idx == 0).then_some("-indentonly")
}

/// The possible arguments of `:messages {clear}` (`get_messages_arg`).
#[must_use]
pub fn get_messages_arg(idx: i32) -> Option<&'static str> {
    (idx == 0).then_some("clear")
}

/// The possible arguments of `:mapclear` (`get_mapclear_arg`).
#[must_use]
pub fn get_mapclear_arg(idx: i32) -> Option<&'static str> {
    (idx == 0).then_some("<buffer>")
}

/// Whether fuzzy matching may be used for this completion context
/// (`cmdline_fuzzy_completion_supported`).
///
/// A group of contexts opt OUT unconditionally: they either match
/// against the filesystem or have their own ordering that fuzzy
/// matching would scramble. Everything else defers to the `"fuzzy"`
/// flag in `'wildoptions'`.
///
/// The original takes the whole `expand_T`; only `xp_context` is
/// read, so that is all this takes.
#[must_use]
pub fn cmdline_fuzzy_completion_supported(
    xp_context: crate::cmdexpand_defs::ExpandContext,
) -> bool {
    use crate::cmdexpand_defs::ExpandContext as E;
    if matches!(
        xp_context,
        E::BoolSettings
            | E::Colors
            | E::Compiler
            | E::Directories
            | E::DirsInCdpath
            | E::Files
            | E::FilesInPath
            | E::Filetype
            | E::Filetypecmd
            | E::Findfunc
            | E::Help
            | E::Keymap
            | E::Lua
            | E::OldSetting
            | E::StringSetting
            | E::SettingSubtract
            | E::Ownsyntax
            | E::Packadd
            | E::Runtime
            | E::Shellcmd
            | E::Shellcmdline
            | E::Tags
            | E::TagsListfiles
            | E::UserList
            | E::UserLua
    ) {
        return false;
    }

    let wop_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.wop_flags;
    wop_flags & crate::option_vars::opt_wop_flag::FUZZY != 0
}

/// Whether fuzzy completion for cmdline completion is enabled AND
/// `fuzzystr` is not empty - an empty search pattern should never use
/// fuzzy matching (`cmdline_fuzzy_complete`).
#[must_use]
pub fn cmdline_fuzzy_complete(fuzzystr: &[u8]) -> bool {
    let wop_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.wop_flags;
    (wop_flags & crate::option_vars::opt_wop_flag::FUZZY) != 0 && !fuzzystr.is_empty()
}

/// Comparator for sorting cmdline completion matches: `<SNR>`-style
/// (or any other `<`-prefixed) names sort to the end; otherwise, a
/// plain lexicographic byte comparison (`sort_func_compare`).
///
/// Returns a negative/zero/positive `i32`, matching `strcmp`'s own
/// convention - this crate's established comparator-function
/// translation shape (e.g. `path::path_fnamencmp`).
#[must_use]
pub fn sort_func_compare(s1: &[u8], s2: &[u8]) -> i32 {
    let p1_is_bracketed = s1.first() == Some(&b'<');
    let p2_is_bracketed = s2.first() == Some(&b'<');
    if !p1_is_bracketed && p2_is_bracketed {
        return -1;
    }
    if p1_is_bracketed && !p2_is_bracketed {
        return 1;
    }
    match s1.cmp(s2) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Saves and restores `'wildoptions'` flags across a test.
    struct WopFlagsGuard {
        saved: u32,
    }

    impl WopFlagsGuard {
        fn set(flags: u32) -> Self {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let saved = opts.wop_flags;
            opts.wop_flags = flags;
            Self { saved }
        }
    }

    impl Drop for WopFlagsGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.wop_flags = self.saved;
        }
    }

    /// Every expected value below was read out of a real `nvim`
    /// binary via `getcompletion('retab ', 'cmdline')` and friends.
    #[test]
    fn get_retab_arg_offers_only_indentonly() {
        assert_eq!(get_retab_arg(0), Some("-indentonly"));
        assert_eq!(get_retab_arg(1), None);
    }

    // --- cmdline_fuzzy_completion_supported ---

    #[test]
    fn fuzzy_completion_is_refused_for_the_opted_out_contexts() {
        use crate::cmdexpand_defs::ExpandContext as E;
        let _lock = crate::globals::global_state_test_lock();
        // Even with 'wildoptions' fuzzy ON, these must refuse.
        let _guard = WopFlagsGuard::set(crate::option_vars::opt_wop_flag::FUZZY);
        for ctx in [
            E::Files,
            E::Directories,
            E::DirsInCdpath,
            E::FilesInPath,
            E::Shellcmd,
            E::Shellcmdline,
            E::Tags,
            E::TagsListfiles,
            E::Help,
            E::Colors,
            E::Compiler,
            E::Filetype,
            E::Filetypecmd,
            E::Findfunc,
            E::Keymap,
            E::Lua,
            E::UserLua,
            E::UserList,
            E::Ownsyntax,
            E::Packadd,
            E::Runtime,
            E::BoolSettings,
            E::OldSetting,
            E::StringSetting,
            E::SettingSubtract,
        ] {
            assert!(
                !cmdline_fuzzy_completion_supported(ctx),
                "{ctx:?} opts out of fuzzy matching"
            );
        }
    }

    #[test]
    fn fuzzy_completion_follows_wildoptions_for_other_contexts() {
        use crate::cmdexpand_defs::ExpandContext as E;
        let _lock = crate::globals::global_state_test_lock();
        {
            let _guard = WopFlagsGuard::set(crate::option_vars::opt_wop_flag::FUZZY);
            assert!(cmdline_fuzzy_completion_supported(E::Commands));
            assert!(cmdline_fuzzy_completion_supported(E::Buffers));
            assert!(cmdline_fuzzy_completion_supported(E::Mappings));
        }
        {
            let _guard = WopFlagsGuard::set(0);
            assert!(!cmdline_fuzzy_completion_supported(E::Commands));
            assert!(!cmdline_fuzzy_completion_supported(E::Buffers));
        }
    }

    #[test]
    fn fuzzy_completion_ignores_unrelated_wildoptions_flags() {
        use crate::cmdexpand_defs::ExpandContext as E;
        let _lock = crate::globals::global_state_test_lock();
        let _guard = WopFlagsGuard::set(crate::option_vars::opt_wop_flag::PUM);
        assert!(!cmdline_fuzzy_completion_supported(E::Commands));
    }

    #[test]
    fn get_messages_arg_offers_only_clear() {
        assert_eq!(get_messages_arg(0), Some("clear"));
        assert_eq!(get_messages_arg(1), None);
    }

    #[test]
    fn get_mapclear_arg_offers_only_buffer() {
        assert_eq!(get_mapclear_arg(0), Some("<buffer>"));
        assert_eq!(get_mapclear_arg(1), None);
    }

    #[test]
    fn the_arg_callbacks_reject_negative_indices() {
        // ExpandGeneric only ever passes idx >= 0, but a negative
        // index must not be mistaken for the single valid entry.
        assert_eq!(get_retab_arg(-1), None);
        assert_eq!(get_messages_arg(-1), None);
        assert_eq!(get_mapclear_arg(-1), None);
    }

    #[test]
    fn cmdline_compl_use_pum_needs_both_wildmenu_and_the_pum_flag() {
        let _lock = crate::globals::global_state_test_lock();
        // ui_has() is always false in this crate today, so the two
        // UI-capability branches never fire and this isolates the
        // 'wildoptions' branch.
        let _wop = WopFlagsGuard::set(crate::option_vars::opt_wop_flag::PUM);
        assert!(unsafe { cmdline_compl_use_pum(true) });
        // The flag alone is not enough without a wildmenu.
        assert!(!unsafe { cmdline_compl_use_pum(false) });
    }

    #[test]
    fn cmdline_compl_use_pum_is_false_without_the_pum_flag() {
        let _lock = crate::globals::global_state_test_lock();
        let _wop = WopFlagsGuard::set(0);
        assert!(!unsafe { cmdline_compl_use_pum(true) });
        assert!(!unsafe { cmdline_compl_use_pum(false) });
    }

    fn set_wop_fuzzy(enabled: bool) -> u32 {
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let old = opts.wop_flags;
        opts.wop_flags = if enabled { crate::option_vars::opt_wop_flag::FUZZY } else { 0 };
        old
    }

    #[test]
    fn cmdline_fuzzy_complete_false_when_disabled() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_wop_fuzzy(false);
        assert!(!cmdline_fuzzy_complete(b"foo"));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.wop_flags = old;
    }

    #[test]
    fn cmdline_fuzzy_complete_true_when_enabled_and_nonempty() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_wop_fuzzy(true);
        assert!(cmdline_fuzzy_complete(b"foo"));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.wop_flags = old;
    }

    #[test]
    fn cmdline_fuzzy_complete_false_when_pattern_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_wop_fuzzy(true);
        assert!(!cmdline_fuzzy_complete(b""));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.wop_flags = old;
    }

    #[test]
    fn sort_func_compare_bracketed_names_sort_last() {
        assert!(sort_func_compare(b"foo", b"<SNR>1_bar") < 0);
        assert!(sort_func_compare(b"<SNR>1_bar", b"foo") > 0);
    }

    #[test]
    fn sort_func_compare_both_bracketed_uses_strcmp() {
        assert_eq!(sort_func_compare(b"<SNR>1_a", b"<SNR>1_a"), 0);
        assert!(sort_func_compare(b"<SNR>1_a", b"<SNR>1_b") < 0);
    }

    #[test]
    fn sort_func_compare_neither_bracketed_uses_strcmp() {
        assert_eq!(sort_func_compare(b"abc", b"abc"), 0);
        assert!(sort_func_compare(b"abc", b"abd") < 0);
        assert!(sort_func_compare(b"abd", b"abc") > 0);
    }
}
