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
//! Also translated: the `compl_match_array` file-static (as
//! `COMPL_MATCH_ARRAY`, over the newly-real
//! [`crate::popupmenu::PumitemT`]) and [`cmdline_pum_active`], plus
//! [`cmdline_compl_pattern`] and [`cmdline_compl_is_fuzzy`], which
//! read the real `ccline.xpc` expansion state.
//!
//! Also translated: the `WILD_*` groups from `cmdexpand.h` (as
//! [`wild_action`] and [`wild_flags`]), the `EW_*` group they map
//! into (in `path.rs`), and [`map_wildopts_to_ewflags`].
//!
//! Also translated: the two `ExpandGeneric()` callbacks driven by a
//! file-static sub-command tag - [`get_filetypecmd_arg`] and
//! [`get_breakadd_arg`], with their [`FILETYPE_EXPAND_WHAT`]/
//! [`BREAKPT_EXPAND_WHAT`] state. Candidate sets verified against a
//! real `nvim` (v0.13.0-dev) via
//! `getcompletion('breakadd ', 'cmdline')` and friends.
//!
//! Deferred: everything else - `nextwild`/`copy_substring_from_pos`/
//! `is_regex_match`/`concat_pattern_with_buffer_match`/
//! `expand_pattern_in_buf` (the completion/search machinery),
//! `wildescape`/`ExpandEscape` (need `vim_strsave_fnameescape`/
//! `escape_fname`/`tilde_replace`, not translated).

/// Set completion context for a `++opt=arg` argument
/// (`set_context_in_argopt`).
#[allow(dead_code)]
fn set_context_in_argopt(
    xp: &mut crate::cmdexpand_defs::ExpandT,
    argument: &[u8],
) {
    let pattern = argument
        .iter()
        .position(|&byte| byte == b'=')
        .map_or(argument, |equals| &argument[equals + 1..]);
    xp.xp_pattern = Some(pattern.to_vec());
    xp.xp_context = crate::cmdexpand_defs::ExpandContext::Argopt;
}

/// Set completion context for `:scriptnames`
/// (`set_context_in_scriptnames_cmd`).
#[allow(dead_code)]
fn set_context_in_scriptnames_cmd(
    xp: &mut crate::cmdexpand_defs::ExpandT,
    argument: &[u8],
) {
    xp.xp_context = crate::cmdexpand_defs::ExpandContext::Nothing;
    xp.xp_pattern = None;
    let argument = &argument[crate::charset::skipwhite(argument)..];
    if argument
        .first()
        .is_some_and(u8::is_ascii_digit)
    {
        return;
    }
    xp.xp_context = crate::cmdexpand_defs::ExpandContext::Scriptnames;
    xp.xp_pattern = Some(argument.to_vec());
}

static CMDLINE_ORIG: crate::globals::GlobalCell<Option<Vec<u8>>> =
    crate::globals::GlobalCell::new(None);

/// Clears the command line saved while completion is active
/// (`clear_cmdline_orig`).
pub fn clear_cmdline_orig() {
    unsafe { *CMDLINE_ORIG.get_mut() = None };
}

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

/// `compl_match_array` - the currently displayed list of cmdline
/// completion entries in the popup menu. `None` when there is no
/// popup menu, matching the original's `NULL`.
///
/// Only ever set by `cmdline_pum_display`'s own array-building loop
/// (needing the real `expand_T` match list, not yet translated) and
/// cleared by `cmdline_pum_remove`, so this stays `None` in this
/// crate today - matching `crate::popupmenu::PUM_IS_VISIBLE`'s own
/// established treatment.
///
/// The original's companion `compl_match_arraysize` has no
/// counterpart: a `Vec` carries its own length.
///
/// Note `Some(Vec::new())` is deliberately reachable and is NOT the
/// same as `None`: the original builds the array with
/// `xmalloc(sizeof(pumitem_T) * numMatches)`, and neovim's `xmalloc`
/// returns a non-NULL pointer even for a zero-size request, so an
/// empty match list still leaves `compl_match_array` non-NULL.
static COMPL_MATCH_ARRAY: crate::globals::GlobalCell<Option<Vec<crate::popupmenu::PumitemT>>> =
    crate::globals::GlobalCell::new(None);

/// Whether the cmdline completion popup menu is currently displayed
/// (`cmdline_pum_active`).
///
/// Both halves matter: the popup menu can be visible for INSERT-mode
/// completion (`insexpand.c`) while no cmdline completion is running,
/// in which case `compl_match_array` is still NULL and this is false.
///
/// # Safety
/// Must not run concurrently with any write to
/// `crate::popupmenu`'s own `pum_is_visible` or to
/// `COMPL_MATCH_ARRAY`.
#[must_use]
pub unsafe fn cmdline_pum_active() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    crate::popupmenu::pum_visible() && unsafe { COMPL_MATCH_ARRAY.get_mut() }.is_some()
}

/// The string cmdline completion is currently expanding
/// (`cmdline_compl_pattern`), or `None` when no cmdline completion
/// state exists.
///
/// The original returns a borrowed `char *` into the live `expand_T`;
/// an owned copy is returned here instead, since the pointer's target
/// is reachable only through a raw pointer in a file-static and the
/// original's own callers use it strictly read-only.
///
/// Both of the original's NULL results collapse into `None`: a NULL
/// `xpc` and a NULL `xp_orig` are indistinguishable to every caller,
/// each of which tests only `leader == NULL`.
///
/// # Safety
/// Touches the `ccline` file-static and, if set, its `xpc` pointer.
#[must_use]
pub unsafe fn cmdline_compl_pattern() -> Option<Vec<u8>> {
    // SAFETY: forwarded from this function's own safety doc.
    let xp = unsafe { (*crate::ex_getln::get_cmdline_info()).xpc };
    if xp.is_null() {
        return None;
    }
    // SAFETY: forwarded from this function's own safety doc; `xp` was
    // just checked non-null.
    unsafe { (*xp).xp_orig.clone() }
}

/// Whether fuzzy cmdline completion is active
/// (`cmdline_compl_is_fuzzy`).
///
/// # Safety
/// Touches the `ccline` file-static and, if set, its `xpc` pointer,
/// plus `crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn cmdline_compl_is_fuzzy() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let xp = unsafe { (*crate::ex_getln::get_cmdline_info()).xpc };
    // SAFETY: forwarded from this function's own safety doc; `xp` is
    // checked non-null before it is read.
    !xp.is_null() && cmdline_fuzzy_completion_supported(unsafe { (*xp).xp_context })
}

/// Values for `nextwild()` and `ExpandOne()` - which step of
/// wildcard expansion to perform. See `ExpandOne()` for meaning.
pub mod wild_action {
    pub const FREE: i32 = 1;
    pub const EXPAND_FREE: i32 = 2;
    pub const EXPAND_KEEP: i32 = 3;
    pub const NEXT: i32 = 4;
    pub const PREV: i32 = 5;
    pub const ALL: i32 = 6;
    pub const LONGEST: i32 = 7;
    pub const ALL_KEEP: i32 = 8;
    pub const CANCEL: i32 = 9;
    pub const APPLY: i32 = 10;
    pub const PAGEUP: i32 = 11;
    pub const PAGEDOWN: i32 = 12;
    pub const PUM_WANT: i32 = 13;
}

/// `WILD_*` option bit flags, passed alongside a [`wild_action`]
/// value.
pub mod wild_flags {
    pub const LIST_NOTFOUND: i32 = 0x01;
    pub const HOME_REPLACE: i32 = 0x02;
    pub const USE_NL: i32 = 0x04;
    pub const NO_BEEP: i32 = 0x08;
    pub const ADD_SLASH: i32 = 0x10;
    pub const KEEP_ALL: i32 = 0x20;
    pub const SILENT: i32 = 0x40;
    pub const ESCAPE: i32 = 0x80;
    pub const ICASE: i32 = 0x100;
    pub const ALLLINKS: i32 = 0x200;
    pub const USE_COMPLETESLASH: i32 = 0x400;
    /// sets `EW_NOERROR` (`WILD_NOERROR`).
    pub const NOERROR: i32 = 0x800;
    pub const BUFLASTUSED: i32 = 0x1000;
    /// `BUF_DIFF_FILTER` - the one member of this group that is not
    /// spelled `WILD_*` in the original.
    pub const BUF_DIFF_FILTER: i32 = 0x2000;
    pub const NOSELECT: i32 = 0x4000;
    pub const MAY_EXPAND_PATTERN: i32 = 0x8000;
    /// called from `wildtrigger()` (`WILD_FUNC_TRIGGER`).
    pub const FUNC_TRIGGER: i32 = 0x10000;
    pub const NOINSERT: i32 = 0x20000;
    pub const USE_SHELLSLASH: i32 = 0x40000;
}

/// Translate the caller's `WILD_*` options into the `EW_*` flags
/// `expand_wildcards()` understands (`map_wildopts_to_ewflags`).
///
/// Only six of the `WILD_*` flags have an `EW_*` counterpart; the
/// rest steer cmdline completion itself and are deliberately dropped
/// here. `EW_DIR` is always set - directories are always included.
#[must_use]
pub fn map_wildopts_to_ewflags(options: i32) -> i32 {
    use crate::path::ew_flags;
    // include directories
    let mut flags = ew_flags::DIR;
    if options & wild_flags::LIST_NOTFOUND != 0 {
        flags |= ew_flags::NOTFOUND;
    }
    if options & wild_flags::ADD_SLASH != 0 {
        flags |= ew_flags::ADDSLASH;
    }
    if options & wild_flags::KEEP_ALL != 0 {
        flags |= ew_flags::KEEPALL;
    }
    if options & wild_flags::SILENT != 0 {
        flags |= ew_flags::SILENT;
    }
    if options & wild_flags::NOERROR != 0 {
        flags |= ew_flags::NOERROR;
    }
    if options & wild_flags::ALLLINKS != 0 {
        flags |= ew_flags::ALLLINKS;
    }
    flags
}

/// Which set of `:filetype` values completion should offer
/// (`filetype_expand_what`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FiletypeExpandWhat {
    /// expand all `:filetype` values (`EXP_FILETYPECMD_ALL`).
    #[default]
    All,
    /// expand `plugin on off` (`EXP_FILETYPECMD_PLUGIN`).
    Plugin,
    /// expand `indent on off` (`EXP_FILETYPECMD_INDENT`).
    Indent,
    /// expand `on off` (`EXP_FILETYPECMD_ONOFF`).
    OnOff,
}

/// Which sub-commands the breakpoint completion should offer
/// (`breakpt_expand_what`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BreakptExpandWhat {
    /// expand `:breakadd` sub-commands (`EXP_BREAKPT_ADD`).
    #[default]
    Add,
    /// expand `:breakdel` sub-commands (`EXP_BREAKPT_DEL`).
    Del,
    /// expand `:profdel` sub-commands (`EXP_PROFDEL`).
    ProfDel,
}

/// `filetype_expand_what` - set by `set_context_in_filetype_cmd`
/// before `ExpandGeneric()` calls [`get_filetypecmd_arg`].
pub static FILETYPE_EXPAND_WHAT: crate::globals::GlobalCell<FiletypeExpandWhat> =
    crate::globals::GlobalCell::new(FiletypeExpandWhat::All);

/// `breakpt_expand_what` - set by `set_context_in_breakadd_cmd`
/// before `ExpandGeneric()` calls [`get_breakadd_arg`].
pub static BREAKPT_EXPAND_WHAT: crate::globals::GlobalCell<BreakptExpandWhat> =
    crate::globals::GlobalCell::new(BreakptExpandWhat::Add);

/// The possible arguments of `:filetype {plugin,indent}`
/// (`get_filetypecmd_arg`), for the sub-command recorded in
/// [`FILETYPE_EXPAND_WHAT`].
///
/// Returns `None` past the end, which is how `ExpandGeneric()` learns
/// to stop.
///
/// # Safety
/// Must not run concurrently with any write to
/// [`FILETYPE_EXPAND_WHAT`].
#[must_use]
pub unsafe fn get_filetypecmd_arg(idx: i32) -> Option<&'static str> {
    if idx < 0 {
        return None;
    }
    let idx = idx as usize;
    // SAFETY: forwarded from this function's own safety doc.
    match unsafe { *FILETYPE_EXPAND_WHAT.get_mut() } {
        FiletypeExpandWhat::All => {
            ["detect", "indent", "plugin", "on", "off"].get(idx).copied()
        }
        FiletypeExpandWhat::Plugin => ["plugin", "on", "off"].get(idx).copied(),
        FiletypeExpandWhat::Indent => ["indent", "on", "off"].get(idx).copied(),
        FiletypeExpandWhat::OnOff => ["on", "off"].get(idx).copied(),
    }
}

/// The possible arguments of `:breakadd`/`:breakdel`/`:profdel`
/// (`get_breakadd_arg`), for the sub-command recorded in
/// [`BREAKPT_EXPAND_WHAT`].
///
/// All three share one four-entry table and differ only in where they
/// start and how far they run: `:breakadd` offers all four,
/// `:breakdel` drops `"expr"`, and `:profdel` drops `"expr"` and
/// `"here"`.
///
/// # Safety
/// Must not run concurrently with any write to
/// [`BREAKPT_EXPAND_WHAT`].
#[must_use]
pub unsafe fn get_breakadd_arg(idx: i32) -> Option<&'static str> {
    if !(0..=3).contains(&idx) {
        return None;
    }
    let opts = ["expr", "file", "func", "here"];
    let idx = idx as usize;
    // SAFETY: forwarded from this function's own safety doc.
    match unsafe { *BREAKPT_EXPAND_WHAT.get_mut() } {
        BreakptExpandWhat::Add => opts.get(idx).copied(),
        // breakdel {func, file, here}
        BreakptExpandWhat::Del => (idx <= 2).then(|| opts[idx + 1]),
        // profdel {func, file}
        BreakptExpandWhat::ProfDel => (idx <= 1).then(|| opts[idx + 1]),
    }
}

/// The tail of a completion match, for display in the wildmenu
/// (`showmatches_gettail`).
///
/// Returns a byte OFFSET into `s` rather than the original's interior
/// pointer, matching this crate's convention (e.g.
/// [`crate::path::path_tail`]).
///
/// With `eager` set, the tail starts after the LAST path separator.
/// Otherwise the tail only moves when a real byte is seen AFTER a
/// separator, so a TRAILING separator never takes effect and the
/// tail stays at the previous component.
#[must_use]
pub fn showmatches_gettail(s: &[u8], eager: bool) -> usize {
    let mut t = 0;
    let mut had_sep = false;

    let mut p = 0;
    while p < s.len() && s[p] != 0 {
        let is_sep = crate::path::vim_ispathsep(i32::from(s[p]))
            // On Windows a backslash may be escaping the next byte
            // rather than separating a path component.
            && !(cfg!(windows) && crate::charset::rem_backslash(&s[p..]));
        if is_sep {
            if eager {
                t = p + 1;
            } else {
                had_sep = true;
            }
        } else if had_sep {
            t = p;
            had_sep = false;
        }
        p += (crate::mbyte::utf_ptr2len(&s[p..]).max(1)) as usize;
    }
    t
}

/// Whether only the tail of completion matches needs to be shown
/// (`expand_showtail`).
///
/// False when not completing file names - where a `"/"` may mean
/// something else entirely - or when the leading path contains a
/// wildcard, since then the matches' directories genuinely differ and
/// the tails alone would be ambiguous.
#[must_use]
pub fn expand_showtail(
    xp_context: crate::cmdexpand_defs::ExpandContext,
    xp_pattern: &[u8],
) -> bool {
    use crate::cmdexpand_defs::ExpandContext as E;
    if !matches!(xp_context, E::Files | E::Shellcmd | E::Directories) {
        return false;
    }

    let end = crate::path::path_tail(xp_pattern);
    if end == 0 {
        // there is no path separator
        return false;
    }

    let mut s = 0;
    while s < end {
        // Skip escaped wildcards. Only when the backslash is not a
        // path separator: on DOS the '*' in "path\*\file" must not be
        // skipped.
        if crate::charset::rem_backslash(&xp_pattern[s..]) {
            s += 2;
        } else if matches!(xp_pattern[s], b'*' | b'?' | b'[') {
            return false;
        } else {
            s += 1;
        }
    }
    true
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

    #[test]
    fn set_context_in_argopt_completes_after_equals_or_from_start() {
        let mut xp = crate::cmdexpand_defs::ExpandT::default();
        set_context_in_argopt(&mut xp, b"encoding=utf");
        assert_eq!(xp.xp_context, crate::cmdexpand_defs::ExpandContext::Argopt);
        assert_eq!(xp.xp_pattern.as_deref(), Some(b"utf".as_slice()));

        set_context_in_argopt(&mut xp, b"encoding");
        assert_eq!(xp.xp_pattern.as_deref(), Some(b"encoding".as_slice()));
    }

    #[test]
    fn set_context_in_scriptnames_cmd_disables_completion_after_a_number() {
        let mut xp = crate::cmdexpand_defs::ExpandT::default();
        set_context_in_scriptnames_cmd(&mut xp, b"  12");
        assert_eq!(xp.xp_context, crate::cmdexpand_defs::ExpandContext::Nothing);
        assert!(xp.xp_pattern.is_none());

        set_context_in_scriptnames_cmd(&mut xp, b"  plugin");
        assert_eq!(
            xp.xp_context,
            crate::cmdexpand_defs::ExpandContext::Scriptnames
        );
        assert_eq!(xp.xp_pattern.as_deref(), Some(b"plugin".as_slice()));

        set_context_in_scriptnames_cmd(&mut xp, b"");
        assert_eq!(
            xp.xp_context,
            crate::cmdexpand_defs::ExpandContext::Scriptnames
        );
        assert_eq!(xp.xp_pattern.as_deref(), Some(b"".as_slice()));
    }

    #[test]
    fn clear_cmdline_orig_releases_the_saved_command_line() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CMDLINE_ORIG.get_mut() = Some(b":set nu".to_vec()) };
        clear_cmdline_orig();
        assert!(unsafe { CMDLINE_ORIG.get_mut() }.is_none());
    }

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

    /// Installs a `COMPL_MATCH_ARRAY` value for the duration of a
    /// test and restores the previous one on drop, even through a
    /// panic.
    struct ComplMatchArrayGuard {
        saved: Option<Vec<crate::popupmenu::PumitemT>>,
    }

    impl ComplMatchArrayGuard {
        fn set(value: Option<Vec<crate::popupmenu::PumitemT>>) -> Self {
            let slot = unsafe { COMPL_MATCH_ARRAY.get_mut() };
            let saved = std::mem::replace(slot, value);
            Self { saved }
        }
    }

    impl Drop for ComplMatchArrayGuard {
        fn drop(&mut self) {
            *unsafe { COMPL_MATCH_ARRAY.get_mut() } = self.saved.take();
        }
    }

    // --- cmdline_pum_active ---

    #[test]
    fn cmdline_pum_is_inactive_with_no_match_array_even_when_the_pum_is_visible() {
        let _lock = crate::globals::global_state_test_lock();
        let _pum = crate::popupmenu::tests::PumVisibleGuard::set(true);
        let _arr = ComplMatchArrayGuard::set(None);
        // The popup menu is visible for INSERT-mode completion; no
        // cmdline completion is running, so this must be false.
        assert!(!unsafe { cmdline_pum_active() });
    }

    #[test]
    fn cmdline_pum_is_inactive_with_a_match_array_when_the_pum_is_not_visible() {
        let _lock = crate::globals::global_state_test_lock();
        let _pum = crate::popupmenu::tests::PumVisibleGuard::set(false);
        let _arr = ComplMatchArrayGuard::set(Some(vec![crate::popupmenu::PumitemT {
            pum_text: b"match".to_vec(),
            ..crate::popupmenu::PumitemT::default()
        }]));
        assert!(!unsafe { cmdline_pum_active() });
    }

    #[test]
    fn cmdline_pum_is_active_only_when_both_hold() {
        let _lock = crate::globals::global_state_test_lock();
        let _pum = crate::popupmenu::tests::PumVisibleGuard::set(true);
        let _arr = ComplMatchArrayGuard::set(Some(vec![crate::popupmenu::PumitemT {
            pum_text: b"match".to_vec(),
            ..crate::popupmenu::PumitemT::default()
        }]));
        assert!(unsafe { cmdline_pum_active() });
    }

    #[test]
    fn an_empty_match_array_still_counts_as_present() {
        let _lock = crate::globals::global_state_test_lock();
        let _pum = crate::popupmenu::tests::PumVisibleGuard::set(true);
        // The original allocates with xmalloc(sizeof(pumitem_T) * 0),
        // which returns a NON-NULL pointer, so a zero-length match
        // list still leaves compl_match_array != NULL. Testing
        // emptiness instead of presence would wrongly report false.
        let _arr = ComplMatchArrayGuard::set(Some(Vec::new()));
        assert!(unsafe { cmdline_pum_active() });
    }

    /// Installs a boxed `ExpandT` as `ccline.xpc` for the duration of
    /// a test, restoring the previous pointer and reclaiming the box
    /// on drop - even through a panic, so a failing test cannot leave
    /// a dangling pointer in the `ccline` file-static for whichever
    /// test runs next.
    ///
    /// The `ExpandT` is boxed (never a stack local) because its
    /// address is stored in a global.
    struct XpcGuard {
        saved: *mut crate::cmdexpand_defs::ExpandT,
        installed: *mut crate::cmdexpand_defs::ExpandT,
    }

    impl XpcGuard {
        fn install(xp: crate::cmdexpand_defs::ExpandT) -> Self {
            let installed = Box::into_raw(Box::new(xp));
            let ccline = unsafe { crate::ex_getln::get_cmdline_info() };
            let saved = unsafe { (*ccline).xpc };
            unsafe { (*ccline).xpc = installed };
            Self { saved, installed }
        }
    }

    impl Drop for XpcGuard {
        fn drop(&mut self) {
            let ccline = unsafe { crate::ex_getln::get_cmdline_info() };
            unsafe { (*ccline).xpc = self.saved };
            drop(unsafe { Box::from_raw(self.installed) });
        }
    }

    // --- cmdline_compl_pattern / cmdline_compl_is_fuzzy ---

    #[test]
    fn compl_pattern_is_absent_with_no_expansion_state() {
        let _lock = crate::globals::global_state_test_lock();
        // ccline.xpc is NULL by default: no cmdline completion state.
        assert_eq!(unsafe { cmdline_compl_pattern() }, None);
    }

    #[test]
    fn compl_pattern_reports_the_originally_expanded_string() {
        let _lock = crate::globals::global_state_test_lock();
        let _xpc = XpcGuard::install(crate::cmdexpand_defs::ExpandT {
            xp_orig: Some(b"col".to_vec()),
            ..crate::cmdexpand_defs::ExpandT::default()
        });
        assert_eq!(unsafe { cmdline_compl_pattern() }, Some(b"col".to_vec()));
    }

    #[test]
    fn compl_pattern_is_absent_when_the_state_exists_but_holds_no_original() {
        let _lock = crate::globals::global_state_test_lock();
        // A non-NULL xpc with a NULL xp_orig must still read as
        // absent - the original returns NULL in both cases.
        let _xpc = XpcGuard::install(crate::cmdexpand_defs::ExpandT::default());
        assert_eq!(unsafe { cmdline_compl_pattern() }, None);
    }

    #[test]
    fn compl_is_not_fuzzy_with_no_expansion_state() {
        let _lock = crate::globals::global_state_test_lock();
        // Even with 'wildoptions' fuzzy ON, a NULL xpc means no
        // cmdline completion is running at all.
        let _guard = WopFlagsGuard::set(crate::option_vars::opt_wop_flag::FUZZY);
        assert!(!unsafe { cmdline_compl_is_fuzzy() });
    }

    #[test]
    fn compl_is_fuzzy_follows_the_context_when_state_exists() {
        use crate::cmdexpand_defs::ExpandContext as E;
        let _lock = crate::globals::global_state_test_lock();
        let _guard = WopFlagsGuard::set(crate::option_vars::opt_wop_flag::FUZZY);

        // A context that opts out of fuzzy matching stays non-fuzzy
        // even with the 'wildoptions' flag on...
        let opted_out = XpcGuard::install(crate::cmdexpand_defs::ExpandT {
            xp_context: E::Files,
            ..crate::cmdexpand_defs::ExpandT::default()
        });
        assert!(!unsafe { cmdline_compl_is_fuzzy() });
        drop(opted_out);

        // ...while a context that defers to 'wildoptions' is fuzzy.
        let _deferring = XpcGuard::install(crate::cmdexpand_defs::ExpandT {
            xp_context: E::Commands,
            ..crate::cmdexpand_defs::ExpandT::default()
        });
        assert!(unsafe { cmdline_compl_is_fuzzy() });
    }

    #[test]
    fn compl_is_not_fuzzy_when_wildoptions_omits_it() {
        use crate::cmdexpand_defs::ExpandContext as E;
        let _lock = crate::globals::global_state_test_lock();
        let _guard = WopFlagsGuard::set(0);
        let _xpc = XpcGuard::install(crate::cmdexpand_defs::ExpandT {
            xp_context: E::Commands,
            ..crate::cmdexpand_defs::ExpandT::default()
        });
        assert!(!unsafe { cmdline_compl_is_fuzzy() });
    }

    // --- showmatches_gettail / expand_showtail ---

    #[test]
    fn gettail_eager_starts_after_the_last_separator() {
        let s = b"one/two/three";
        assert_eq!(&s[showmatches_gettail(s, true)..], b"three");
    }

    #[test]
    fn gettail_returns_the_whole_string_without_a_separator() {
        let s = b"plain";
        assert_eq!(showmatches_gettail(s, true), 0);
        assert_eq!(showmatches_gettail(s, false), 0);
    }

    /// A TRAILING separator is where the two modes genuinely diverge,
    /// hand-traced against the original: eager jumps past it, leaving
    /// an empty tail, while non-eager only commits the tail when it
    /// sees a real byte AFTER a separator - so the trailing one never
    /// takes effect and the tail stays at the previous component.
    #[test]
    fn gettail_handles_a_trailing_separator_differently_per_mode() {
        let s = b"one/two/";
        assert_eq!(&s[showmatches_gettail(s, false)..], b"two/");
        assert_eq!(showmatches_gettail(s, true), s.len());
    }

    /// The two modes genuinely differ on a run of separators: eager
    /// jumps past the LAST one immediately, while non-eager only
    /// commits when it sees the next real byte. Both land in the same
    /// place here, but the intermediate handling differs, so this
    /// checks a case with content after the run.
    #[test]
    fn gettail_skips_a_run_of_separators_in_both_modes() {
        let s = b"a//b";
        assert_eq!(&s[showmatches_gettail(s, true)..], b"b");
        assert_eq!(&s[showmatches_gettail(s, false)..], b"b");
    }

    #[test]
    fn gettail_advances_over_multibyte_characters() {
        // A 3-byte character must be stepped over whole; walking it
        // byte-by-byte could mistake a continuation byte for a
        // separator on some inputs.
        let s = "dir/\u{4f60}\u{597d}".as_bytes();
        assert_eq!(&s[showmatches_gettail(s, true)..], "\u{4f60}\u{597d}".as_bytes());
    }

    #[test]
    fn showtail_is_false_when_not_completing_file_names() {
        use crate::cmdexpand_defs::ExpandContext as E;
        // Same pattern that would otherwise qualify.
        assert!(!expand_showtail(E::Commands, b"dir/file"));
        assert!(!expand_showtail(E::Colors, b"dir/file"));
    }

    #[test]
    fn showtail_is_true_for_a_plain_directory_prefix() {
        use crate::cmdexpand_defs::ExpandContext as E;
        for ctx in [E::Files, E::Shellcmd, E::Directories] {
            assert!(expand_showtail(ctx, b"dir/file"), "{ctx:?}");
        }
    }

    /// Without a path separator there is no directory prefix to hide,
    /// so the tail is not shown.
    #[test]
    fn showtail_is_false_without_a_path_separator() {
        use crate::cmdexpand_defs::ExpandContext as E;
        assert!(!expand_showtail(E::Files, b"file"));
    }

    /// A wildcard in the LEADING path means the matches come from
    /// genuinely different directories, so their tails alone would be
    /// ambiguous.
    #[test]
    fn showtail_is_false_with_a_wildcard_in_the_leading_path() {
        use crate::cmdexpand_defs::ExpandContext as E;
        assert!(!expand_showtail(E::Files, b"d*r/file"));
        assert!(!expand_showtail(E::Files, b"d?r/file"));
        assert!(!expand_showtail(E::Files, b"d[ab]r/file"));
    }

    /// A wildcard in the TAIL is fine - only the leading path is
    /// scanned. An implementation scanning the whole pattern would
    /// wrongly refuse here.
    #[test]
    fn showtail_ignores_a_wildcard_in_the_tail() {
        use crate::cmdexpand_defs::ExpandContext as E;
        assert!(expand_showtail(E::Files, b"dir/fi*le"));
    }

    // --- get_filetypecmd_arg / get_breakadd_arg ---

    /// Installs a `FILETYPE_EXPAND_WHAT` value and restores it on
    /// drop, even through a panic.
    struct FiletypeWhatGuard(FiletypeExpandWhat);

    impl FiletypeWhatGuard {
        fn set(v: FiletypeExpandWhat) -> Self {
            let cell = unsafe { FILETYPE_EXPAND_WHAT.get_mut() };
            let me = Self(*cell);
            *cell = v;
            me
        }
    }

    impl Drop for FiletypeWhatGuard {
        fn drop(&mut self) {
            *unsafe { FILETYPE_EXPAND_WHAT.get_mut() } = self.0;
        }
    }

    /// Installs a `BREAKPT_EXPAND_WHAT` value and restores it on
    /// drop, even through a panic.
    struct BreakptWhatGuard(BreakptExpandWhat);

    impl BreakptWhatGuard {
        fn set(v: BreakptExpandWhat) -> Self {
            let cell = unsafe { BREAKPT_EXPAND_WHAT.get_mut() };
            let me = Self(*cell);
            *cell = v;
            me
        }
    }

    impl Drop for BreakptWhatGuard {
        fn drop(&mut self) {
            *unsafe { BREAKPT_EXPAND_WHAT.get_mut() } = self.0;
        }
    }

    fn collect_filetypecmd_args() -> Vec<&'static str> {
        (0..).map_while(|i| unsafe { get_filetypecmd_arg(i) }).collect()
    }

    fn collect_breakadd_args() -> Vec<&'static str> {
        (0..).map_while(|i| unsafe { get_breakadd_arg(i) }).collect()
    }

    /// The candidate SETS below were read out of a real `nvim`
    /// (v0.13.0-dev) via `getcompletion('breakadd ', 'cmdline')` and
    /// friends. `getcompletion()` sorts its result, so the ORDER
    /// asserted here comes from the original's own tables instead.
    #[test]
    fn breakadd_offers_every_breakpoint_kind() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = BreakptWhatGuard::set(BreakptExpandWhat::Add);
        assert_eq!(collect_breakadd_args(), ["expr", "file", "func", "here"]);
    }

    /// `:breakdel` drops "expr" - it starts one entry INTO the shared
    /// table rather than at the top of it.
    #[test]
    fn breakdel_drops_the_expr_kind() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = BreakptWhatGuard::set(BreakptExpandWhat::Del);
        assert_eq!(collect_breakadd_args(), ["file", "func", "here"]);
    }

    /// `:profdel` drops both "expr" and "here" - same shifted start
    /// as `:breakdel`, but one entry shorter.
    #[test]
    fn profdel_drops_the_expr_and_here_kinds() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = BreakptWhatGuard::set(BreakptExpandWhat::ProfDel);
        assert_eq!(collect_breakadd_args(), ["file", "func"]);
    }

    #[test]
    fn breakadd_arg_refuses_a_negative_index() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = BreakptWhatGuard::set(BreakptExpandWhat::Add);
        assert_eq!(unsafe { get_breakadd_arg(-1) }, None);
    }

    #[test]
    fn filetype_cmd_offers_every_value_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = FiletypeWhatGuard::set(FiletypeExpandWhat::All);
        assert_eq!(
            collect_filetypecmd_args(),
            ["detect", "indent", "plugin", "on", "off"]
        );
    }

    /// Once a sub-command is known, the remaining candidates are the
    /// ones that still make sense after it.
    #[test]
    fn filetype_cmd_narrows_to_the_pending_subcommand() {
        let _lock = crate::globals::global_state_test_lock();

        let g = FiletypeWhatGuard::set(FiletypeExpandWhat::Plugin);
        assert_eq!(collect_filetypecmd_args(), ["plugin", "on", "off"]);
        drop(g);

        let g = FiletypeWhatGuard::set(FiletypeExpandWhat::Indent);
        assert_eq!(collect_filetypecmd_args(), ["indent", "on", "off"]);
        drop(g);

        let _g = FiletypeWhatGuard::set(FiletypeExpandWhat::OnOff);
        assert_eq!(collect_filetypecmd_args(), ["on", "off"]);
    }

    #[test]
    fn filetypecmd_arg_refuses_a_negative_index() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = FiletypeWhatGuard::set(FiletypeExpandWhat::All);
        assert_eq!(unsafe { get_filetypecmd_arg(-1) }, None);
    }

    // --- map_wildopts_to_ewflags ---

    /// EW_DIR is unconditional: directories are always included, even
    /// with no options at all.
    #[test]
    fn wildopts_always_include_directories() {
        assert_eq!(map_wildopts_to_ewflags(0), crate::path::ew_flags::DIR);
    }

    #[test]
    fn wildopts_map_each_translated_flag_to_its_ew_counterpart() {
        use crate::path::ew_flags;
        for (wild, ew) in [
            (wild_flags::LIST_NOTFOUND, ew_flags::NOTFOUND),
            (wild_flags::ADD_SLASH, ew_flags::ADDSLASH),
            (wild_flags::KEEP_ALL, ew_flags::KEEPALL),
            (wild_flags::SILENT, ew_flags::SILENT),
            (wild_flags::NOERROR, ew_flags::NOERROR),
            (wild_flags::ALLLINKS, ew_flags::ALLLINKS),
        ] {
            assert_eq!(map_wildopts_to_ewflags(wild), ew_flags::DIR | ew);
        }
    }

    /// The `WILD_*` and `EW_*` bit values are NOT the same, so a
    /// pass-through implementation must fail. `WILD_SILENT` is 0x40
    /// but `EW_SILENT` is 0x20, and `WILD_ALLLINKS` is 0x200 while
    /// `EW_ALLLINKS` is 0x1000.
    #[test]
    fn wildopts_are_translated_rather_than_passed_through() {
        assert_ne!(wild_flags::SILENT, crate::path::ew_flags::SILENT);
        assert_ne!(wild_flags::ALLLINKS, crate::path::ew_flags::ALLLINKS);
        assert_eq!(
            map_wildopts_to_ewflags(wild_flags::SILENT),
            crate::path::ew_flags::DIR | crate::path::ew_flags::SILENT
        );
    }

    /// Flags that steer cmdline completion itself have no `EW_*`
    /// counterpart and must be dropped, not leaked into the result.
    #[test]
    fn wildopts_drop_the_untranslated_flags() {
        let untranslated = wild_flags::HOME_REPLACE
            | wild_flags::USE_NL
            | wild_flags::NO_BEEP
            | wild_flags::ESCAPE
            | wild_flags::ICASE
            | wild_flags::USE_COMPLETESLASH
            | wild_flags::BUFLASTUSED
            | wild_flags::NOSELECT
            | wild_flags::NOINSERT;
        assert_eq!(
            map_wildopts_to_ewflags(untranslated),
            crate::path::ew_flags::DIR
        );
    }

    #[test]
    fn wildopts_combine_several_flags_at_once() {
        use crate::path::ew_flags;
        let got = map_wildopts_to_ewflags(
            wild_flags::LIST_NOTFOUND | wild_flags::SILENT | wild_flags::HOME_REPLACE,
        );
        assert_eq!(
            got,
            ew_flags::DIR | ew_flags::NOTFOUND | ew_flags::SILENT
        );
    }

    /// Every value in each group must be distinct - a duplicated bit
    /// would silently conflate two flags.
    #[test]
    fn wild_flag_values_are_all_distinct() {
        let all = [
            wild_flags::LIST_NOTFOUND,
            wild_flags::HOME_REPLACE,
            wild_flags::USE_NL,
            wild_flags::NO_BEEP,
            wild_flags::ADD_SLASH,
            wild_flags::KEEP_ALL,
            wild_flags::SILENT,
            wild_flags::ESCAPE,
            wild_flags::ICASE,
            wild_flags::ALLLINKS,
            wild_flags::USE_COMPLETESLASH,
            wild_flags::NOERROR,
            wild_flags::BUFLASTUSED,
            wild_flags::BUF_DIFF_FILTER,
            wild_flags::NOSELECT,
            wild_flags::MAY_EXPAND_PATTERN,
            wild_flags::FUNC_TRIGGER,
            wild_flags::NOINSERT,
            wild_flags::USE_SHELLSLASH,
        ];
        let mut seen = 0i32;
        for f in all {
            assert!(f.count_ones() == 1, "{f:#x} is not a single bit");
            assert_eq!(seen & f, 0, "{f:#x} duplicates an earlier flag");
            seen |= f;
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
