//! Translated from `src/nvim/insexpand.c` (tractable core only).
//!
//! `insexpand.c` (~5500 lines) is the real insert-mode completion
//! engine (`i_CTRL-N`/`i_CTRL-P`/omni-completion/etc.) - almost every
//! function needs the popup menu, the completion-source dispatch
//! machinery, and real buffer/cursor mutation, none translated.
//!
//! Translated: the `CTRL_X_*` mode constants and the whole
//! `ctrl_x_mode_*` predicate family (19 pure, `FUNC_ATTR_PURE`
//! functions checking the current Ctrl-X completion sub-mode) plus
//! the `CTRL_X_MODE` file-static itself (`ctrl_x_mode`), and now
//! [`ins_compl_active`] plus its own backing `COMPL_STARTED`
//! file-static (`compl_started`). All are small, self-contained,
//! no-design-freedom equality checks - translated ahead of their real
//! callers (`ins_ctrl_x`/the whole completion-source dispatch, none
//! translated), matching this crate's established "translate a small,
//! simple, mechanically-correct piece ahead of the surrounding engine"
//! precedent.
//!
//! Since `ins_ctrl_x` (the only real mutator of `ctrl_x_mode`) isn't
//! translated, `CTRL_X_MODE` stays `CTRL_X_NORMAL` (its own real
//! static initializer value) forever in this crate today, and
//! `COMPL_STARTED` likewise stays `false` forever (its own only real
//! mutator is the same not-yet-translated completion engine) -
//! exactly matching `state.rs`'s own already-documented assumption
//! for `get_mode`'s `ins_compl_active()`/
//! `ctrl_x_mode_not_defined_yet()` checks (see that function's own
//! doc comment) - `state.rs`'s own `get_mode` has now been refined to
//! call these 2 real predicates directly instead of its own
//! hardcoded-false assumption, since both now exist for real.
//!
//! Also translated: [`pum_wanted`]/[`ins_compl_has_preinsert`]/
//! [`get_compl_len`]. `ins_compl_has_preinsert` needs a DIFFERENT
//! flag combination per mode - without autocomplete it wants
//! `preinsert` AND `menuone` and NOT `fuzzy`; with autocomplete the
//! `menuone` requirement drops, and it is disabled outright while
//! `'ignorecase'` is on without `'infercase'`. Neither branch is
//! simply "is preinsert set".
//!
//! Also translated: [`ins_compl_leader`]/[`ins_compl_leader_len`] and
//! the `compl_leader`/`compl_orig_text` statics. These are
//! `Option<Vec<u8>>` because the original tests `.data != NULL`
//! rather than the size, so a leader that is SET BUT EMPTY still wins
//! over the original text - `None` and `Some(vec![])` are genuinely
//! different states here, the same distinction `cmdhist.rs` needs for
//! its own `hisstr`.
//!
//! Also translated: [`ins_compl_refresh_always`]/
//! [`ins_compl_need_restart`]/[`ins_compl_has_autocomplete`]. Note
//! `'autocomplete'` uses a NEGATIVE buffer-local value as its "unset"
//! marker, so a local `0` is a real "off" - unlike `'completeopt'`,
//! where `0` itself means unset (see [`get_cot_flags`]).
//!
//! Also translated: the key-mapping trio [`ins_compl_key2dir`]/
//! [`ins_compl_pum_key`]/[`ins_compl_key2count`], plus the
//! `compl_selected_item` static and `popupmenu.rs`'s `pum_want`. For
//! the externally-driven keys (`K_EVENT`/`K_COMMAND`/`K_LUA`) the
//! direction and the count are BOTH derived from where the requested
//! item sits relative to the current selection, rather than being
//! fixed by the key - `key2count` returns the absolute distance and
//! `key2dir` carries the sign.
//!
//! Also translated: the word-scanning trio [`find_word_start`]/
//! [`find_word_end`]/[`find_line_end`], returning byte offsets rather
//! than advanced pointers. `find_word_end` guards its whole scan
//! behind `start_class > 1`, so starting off a word returns the start
//! rather than running forward to find one - it assumes it is already
//! inside a word. `find_line_end` scans to the NUL terminator, so an
//! embedded NUL ends the line.
//!
//! Also translated: the completion-state accessors
//! [`ins_compl_used_match`]/[`ins_compl_init_get_longest`]/
//! [`ins_compl_interrupted`]/[`ins_compl_enter_selects`]/
//! [`ins_compl_col`]/[`ins_compl_len`] and their backing statics.
//! Note `ins_compl_interrupted` is an OR of two separate conditions -
//! an explicit interruption AND the current source running out of its
//! time budget - so it is not a plain accessor.
//!
//! Also translated: [`get_cot_flags`] and the `'completeopt'`
//! predicates over it - [`cot_fuzzy`]/[`is_nearest_active`]/
//! [`ins_compl_preinsert_longest`] - plus the `compl_autocomplete`
//! static they consult. Note `ins_compl_preinsert_longest` masks
//! `longest|preinsert|fuzzy` and compares the WHOLE result against
//! `longest`, so it is true only when `longest` is set WITHOUT either
//! companion - not merely when `longest` is present.
//!
//! Also translated: the completion-continuation state
//! (`compl_cont_status` with its `CONT_*` flags, plus
//! `compl_direction`/`compl_shows_dir`) and the small predicates over
//! it - [`compl_status_adding`]/[`compl_status_sol`]/
//! [`compl_status_local`]/[`compl_status_clear`]/[`compl_dir_forward`]/
//! [`compl_shows_dir_forward`]/[`compl_shows_dir_backward`]. These
//! stay at their initial values today for the same reason as
//! `CTRL_X_MODE`/`COMPL_STARTED` above. Note
//! `compl_shows_dir_backward` is NOT the negation of
//! `compl_shows_dir_forward`: `Direction` also carries the
//! `FORWARD_FILE`/`BACKWARD_FILE` values, so both can be false.
//!
//! Also translated: [`set_ref_in_cpt_callbacks`]/
//! [`set_ref_in_insexpand_funcs`] - mark the global `'completefunc'`/
//! `'omnifunc'`/`'thesaurusfunc'`/`'complete'`-`F{func}` callbacks with
//! a GC `copy_id` so they survive garbage collection, via
//! `eval/eval.rs`'s `set_ref_in_callback`. Every one of `CFU_CB`/
//! `OFU_CB`/`TSRFU_CB`/`CPT_CB` stays at its own empty default forever
//! today (see each one's own doc comment) - matches every real,
//! unconfigured session.
//!
//! Also translated, from `insexpand.h` (not `insexpand.c` - a tiny,
//! self-contained enum needed by `popupmenu.c`'s `pum_align_order`):
//! [`CPT_ABBR`]/[`CPT_KIND`]/[`CPT_MENU`]/[`CPT_INFO`]/[`CPT_COUNT`].
//!
//! Also translated: [`ins_compl_arm_autocomplete_delay`] and its
//! pending/start-time state, the timer-arm half of
//! `'autocompletedelay'`.
//!
//! Deferred: everything else in the file.

use crate::globals::GlobalCell;

/// CTRL-N CTRL-P completion, default (`CTRL_X_NORMAL`).
pub const CTRL_X_NORMAL: i32 = 0;
/// `CTRL_X_NOT_DEFINED_YET`.
pub const CTRL_X_NOT_DEFINED_YET: i32 = 1;
/// `CTRL_X_SCROLL`.
pub const CTRL_X_SCROLL: i32 = 2;
/// `CTRL_X_WHOLE_LINE`.
pub const CTRL_X_WHOLE_LINE: i32 = 3;
/// `CTRL_X_FILES`.
pub const CTRL_X_FILES: i32 = 4;
/// Bit indicating the mode wants an identifier character class
/// (`CTRL_X_WANT_IDENT`).
const CTRL_X_WANT_IDENT: i32 = 0x100;
/// `CTRL_X_TAGS`.
pub const CTRL_X_TAGS: i32 = 5 + CTRL_X_WANT_IDENT;
/// `CTRL_X_PATH_PATTERNS`.
pub const CTRL_X_PATH_PATTERNS: i32 = 6 + CTRL_X_WANT_IDENT;
/// `CTRL_X_PATH_DEFINES`.
pub const CTRL_X_PATH_DEFINES: i32 = 7 + CTRL_X_WANT_IDENT;
/// `CTRL_X_FINISHED`.
pub const CTRL_X_FINISHED: i32 = 8;
/// `CTRL_X_DICTIONARY`.
pub const CTRL_X_DICTIONARY: i32 = 9 + CTRL_X_WANT_IDENT;
/// `CTRL_X_THESAURUS`.
pub const CTRL_X_THESAURUS: i32 = 10 + CTRL_X_WANT_IDENT;
/// `CTRL_X_CMDLINE`.
pub const CTRL_X_CMDLINE: i32 = 11;
/// `CTRL_X_FUNCTION`.
pub const CTRL_X_FUNCTION: i32 = 12;
/// `CTRL_X_OMNI`.
pub const CTRL_X_OMNI: i32 = 13;
/// `CTRL_X_SPELL`.
pub const CTRL_X_SPELL: i32 = 14;
/// Only used in `ctrl_x_msgs` (`CTRL_X_LOCAL_MSG`).
pub const CTRL_X_LOCAL_MSG: i32 = 15;
/// For the builtin `complete()` function (`CTRL_X_EVAL`).
pub const CTRL_X_EVAL: i32 = 16;
/// CTRL-X typed in [`CTRL_X_CMDLINE`] mode (`CTRL_X_CMDLINE_CTRL_X`).
pub const CTRL_X_CMDLINE_CTRL_X: i32 = 17;
/// `CTRL_X_BUFNAMES`.
pub const CTRL_X_BUFNAMES: i32 = 18;
/// Complete words from registers (`CTRL_X_REGISTER`).
pub const CTRL_X_REGISTER: i32 = 19;

/// Indices into a completion match's own `cp_text` array, and into
/// `'completeitemalign'`'s own display-order array (`CPT_*`,
/// `insexpand.h`). Kept as `i32` (matching the original's own plain C
/// `enum`, implicitly `int`) rather than `usize`, since real callers
/// use these both as array indices and in `int`-typed arithmetic
/// (e.g. `popupmenu.rs`'s `pum_align_order`, comparing against
/// `cia_flags / 100`).
pub const CPT_ABBR: i32 = 0;
/// (`CPT_KIND`).
pub const CPT_KIND: i32 = 1;
/// (`CPT_MENU`).
pub const CPT_MENU: i32 = 2;
/// (`CPT_INFO`).
pub const CPT_INFO: i32 = 3;
/// Number of `CPT_*` entries (`CPT_COUNT`).
pub const CPT_COUNT: i32 = 4;

/// Flags on a completion match (`cp_flags_T`).
pub mod cp_flags {
    /// the original text, from when the expansion began
    /// (`CP_ORIGINAL_TEXT`).
    pub const ORIGINAL_TEXT: i32 = 1;
    /// `cp_fname` is allocated (`CP_FREE_FNAME`).
    pub const FREE_FNAME: i32 = 2;
    /// use `CONT_S_IPOS` for `compl_cont_status` (`CP_CONT_S_IPOS`).
    pub const CONT_S_IPOS: i32 = 4;
    /// `ins_compl_equal()` always returns true (`CP_EQUAL`).
    pub const EQUAL: i32 = 8;
    /// `ins_compl_equal` ignores case (`CP_ICASE`).
    pub const ICASE: i32 = 16;
    /// use `fast_breakcheck` instead of `os_breakcheck` (`CP_FAST`).
    pub const FAST: i32 = 32;
}

/// One Insert-mode completion match (`compl_T`/`struct compl_S`).
///
/// The matches form an intrusive, circular doubly-linked list, so
/// `cp_next`/`cp_prev`/`cp_match_next` stay raw pointers - this
/// crate's convention for intrusive lists, and the same treatment
/// already given to the buffer and window chains.
///
/// The owned text fields become owned `Option<Vec<u8>>`. Note
/// `cp_fname` is owned only when `cp_flags` has
/// [`cp_flags::FREE_FNAME`]; modelling it as owned regardless is
/// safe because the borrowed case merely copies a little more, and
/// there is no way to express "sometimes borrowed" here without
/// tying every match's lifetime to the source that produced it.
#[derive(Debug)]
pub struct ComplT {
    pub cp_next: *mut ComplT,
    pub cp_prev: *mut ComplT,
    /// matched next `ComplT` (`cp_match_next`).
    pub cp_match_next: *mut ComplT,
    /// matched text (`cp_str`).
    pub cp_str: Option<Vec<u8>>,
    /// text for the menu, indexed by the `CPT_*` constants
    /// (`cp_text`).
    pub cp_text: [Option<Vec<u8>>; CPT_COUNT as usize],
    pub cp_user_data: crate::eval::typval_defs::TypvalT,
    /// file containing the match (`cp_fname`).
    pub cp_fname: Option<Vec<u8>>,
    /// commit characters; may be absent (`cp_commit_chars`).
    pub cp_commit_chars: Option<Vec<u8>>,
    /// [`cp_flags`] values (`cp_flags`).
    pub cp_flags: i32,
    /// sequence number (`cp_number`).
    pub cp_number: i32,
    /// preselect item (`cp_preselect`).
    pub cp_preselect: bool,
    /// fuzzy match score or proximity score (`cp_score`).
    pub cp_score: i32,
    /// collected by `compl_match_array` (`cp_in_match_array`).
    pub cp_in_match_array: bool,
    /// highlight attribute for abbr (`cp_user_abbr_hlattr`).
    pub cp_user_abbr_hlattr: i32,
    /// highlight attribute for kind (`cp_user_kind_hlattr`).
    pub cp_user_kind_hlattr: i32,
    /// index of this match's source in `'complete'`
    /// (`cp_cpt_source_idx`).
    pub cp_cpt_source_idx: i32,
}

impl Default for ComplT {
    fn default() -> Self {
        ComplT {
            cp_next: std::ptr::null_mut(),
            cp_prev: std::ptr::null_mut(),
            cp_match_next: std::ptr::null_mut(),
            cp_str: None,
            cp_text: [None, None, None, None],
            cp_user_data: crate::eval::typval_defs::TypvalT::default(),
            cp_fname: None,
            cp_commit_chars: None,
            cp_flags: 0,
            cp_number: 0,
            cp_preselect: false,
            cp_score: 0,
            cp_in_match_array: false,
            cp_user_abbr_hlattr: 0,
            cp_user_kind_hlattr: 0,
            cp_cpt_source_idx: 0,
        }
    }
}

/// The match currently shown in the completion menu
/// (`compl_shown_match`).
///
/// A raw pointer into the intrusive match list, matching the
/// original. Only ever set by the completion-source dispatch
/// machinery (not translated), so this stays null in this crate
/// today - the same treatment as `COMPL_STARTED` and friends.
static COMPL_SHOWN_MATCH: GlobalCell<*mut ComplT> = GlobalCell::new(std::ptr::null_mut());

/// Whether the shown match spans more than one line
/// (`ins_compl_has_multiple`).
///
/// A multi-line match is stored as one string containing newlines,
/// so this is a search for a newline rather than a count.
///
/// # Safety
/// `COMPL_SHOWN_MATCH` must be a valid, non-null pointer - the
/// original dereferences it without checking, so a null there is
/// already a contract violation.
#[must_use]
pub unsafe fn ins_compl_has_multiple() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let shown = unsafe { *COMPL_SHOWN_MATCH.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &*shown }
        .cp_str
        .as_deref()
        .is_some_and(|s| s.contains(&b'\n'))
}

/// Whether there is a shown match to move away from
/// (`ins_compl_has_shown_match`).
///
/// True when no match is shown at all, OR when the shown match is not
/// the only one. A single match links to ITSELF in the circular list,
/// which is what the self-comparison detects.
///
/// # Safety
/// `COMPL_SHOWN_MATCH`, if non-null, must be a valid pointer.
#[must_use]
pub unsafe fn ins_compl_has_shown_match() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let shown = unsafe { *COMPL_SHOWN_MATCH.get_mut() };
    if shown.is_null() {
        return true;
    }
    // SAFETY: forwarded from this function's own safety doc.
    !std::ptr::eq(shown, unsafe { &*shown }.cp_next)
}

/// Whether the shown match is long enough to still cover the text
/// typed so far (`ins_compl_long_shown_match`).
///
/// # Safety
/// `COMPL_SHOWN_MATCH`, if non-null, must be a valid pointer;
/// `GLOBALS.curwin` must be valid and non-null.
#[must_use]
pub unsafe fn ins_compl_long_shown_match() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let shown = unsafe { *COMPL_SHOWN_MATCH.get_mut() };
    if shown.is_null() {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let Some(text) = unsafe { &*shown }.cp_str.as_deref() else {
        return false;
    };
    // SAFETY: forwarded from this function's own safety doc.
    let cursor_col = unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.col;
    // SAFETY: forwarded from this function's own safety doc.
    let compl_col = unsafe { *COMPL_COL.get_mut() };
    i64::try_from(text.len()).unwrap_or(i64::MAX) > i64::from(cursor_col - compl_col)
}

/// Whether `match` is the original text, from before the expansion
/// began (`match_at_original_text`).
#[must_use]
pub fn match_at_original_text(m: &ComplT) -> bool {
    m.cp_flags & cp_flags::ORIGINAL_TEXT != 0
}

/// The next match in the list (`cp_get_next`).
#[must_use]
pub fn cp_get_next(m: &ComplT) -> *mut ComplT {
    m.cp_next
}

/// Link `next` after `m` (`cp_set_next`).
pub fn cp_set_next(m: &mut ComplT, next: *mut ComplT) {
    m.cp_next = next;
}

/// The previous match in the list (`cp_get_prev`).
#[must_use]
pub fn cp_get_prev(m: &ComplT) -> *mut ComplT {
    m.cp_prev
}

/// Link `prev` before `m` (`cp_set_prev`).
pub fn cp_set_prev(m: &mut ComplT, prev: *mut ComplT) {
    m.cp_prev = prev;
}

/// Comparator ordering fuzzy-completion matches (`cp_compare_fuzzy`).
///
/// DESCENDING by score, so the best fuzzy match comes first. Note the
/// original writes its comparisons against `b` first, which is what
/// produces that descending order out of `qsort`.
///
/// Returns a negative/zero/positive `i32`, matching `qsort`'s own
/// convention and this crate's established comparator shape.
#[must_use]
pub fn cp_compare_fuzzy(a: &ComplT, b: &ComplT) -> i32 {
    if b.cp_score > a.cp_score {
        1
    } else if b.cp_score < a.cp_score {
        -1
    } else {
        0
    }
}

/// Comparator ordering matches by proximity (`cp_compare_nearest`).
///
/// ASCENDING by score - the opposite of [`cp_compare_fuzzy`] - so the
/// nearest match comes first.
///
/// A match with no score at all
/// ([`crate::fuzzy::FUZZY_SCORE_NONE`]) compares EQUAL to everything,
/// which leaves such entries where they already are rather than
/// sorting them to one end.
#[must_use]
pub fn cp_compare_nearest(a: &ComplT, b: &ComplT) -> i32 {
    if a.cp_score == crate::fuzzy::FUZZY_SCORE_NONE
        || b.cp_score == crate::fuzzy::FUZZY_SCORE_NONE
    {
        return 0;
    }
    if a.cp_score > b.cp_score {
        1
    } else if a.cp_score < b.cp_score {
        -1
    } else {
        0
    }
}

/// Which Ctrl-X mode are we in? (`ctrl_x_mode`). Always
/// [`CTRL_X_NORMAL`] today - see this module's own doc comment.
static CTRL_X_MODE: GlobalCell<i32> = GlobalCell::new(CTRL_X_NORMAL);

/// Whether Insert-mode completion is currently active (`compl_started`).
/// Always `false` today - nothing in this crate can currently start a
/// real completion session (the only real mutator, `ins_ctrl_x`/the
/// completion-source dispatch machinery, isn't translated), matching
/// [`CTRL_X_MODE`]'s own established treatment exactly.
static COMPL_STARTED: GlobalCell<bool> = GlobalCell::new(false);

/// Whether one of the matches was selected, rather than the text
/// being edited or the longest common string used (`compl_used_match`).
///
/// The original declares this without an initializer, so it starts
/// `false` like the rest.
static COMPL_USED_MATCH: GlobalCell<bool> = GlobalCell::new(false);

/// Whether to put the longest common string in `compl_leader`
/// (`compl_get_longest`).
static COMPL_GET_LONGEST: GlobalCell<bool> = GlobalCell::new(false);

/// Whether insert completion was interrupted (`compl_interrupted`).
static COMPL_INTERRUPTED: GlobalCell<bool> = GlobalCell::new(false);

/// Whether the time budget for the current source was exceeded
/// (`compl_time_slice_expired`).
static COMPL_TIME_SLICE_EXPIRED: GlobalCell<bool> = GlobalCell::new(false);

/// Whether `<Enter>` selects a match in the completion popup menu
/// (`compl_enter_selects`).
static COMPL_ENTER_SELECTS: GlobalCell<bool> = GlobalCell::new(false);

/// Column where the text being completed starts (`compl_col`).
static COMPL_COL: GlobalCell<crate::pos_defs::ColnrT> = GlobalCell::new(0);

/// Length in bytes of the text being completed (`compl_length`).
static COMPL_LENGTH: GlobalCell<i32> = GlobalCell::new(0);

/// Whether the popup menu should be displayed (`pum_wanted`).
///
/// `'completeopt'` must contain `menu` or `menuone`, unless
/// autocomplete is on - which wants the menu regardless.
///
/// # Safety
/// Forwarded from [`get_cot_flags`].
#[must_use]
pub unsafe fn pum_wanted() -> bool {
    use crate::option_vars::opt_cot_flag::{MENU, MENUONE};
    // SAFETY: forwarded from this function's own safety doc.
    if (unsafe { get_cot_flags() }) & (MENU | MENUONE) != 0 {
        return true;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *COMPL_AUTOCOMPLETE.get_mut() }
}

/// Whether `'completeopt'`'s `preinsert` effect is in force
/// (`ins_compl_has_preinsert`).
///
/// The required flag combination differs by mode, and neither branch
/// is simply "is preinsert set": without autocomplete it needs
/// `preinsert` AND `menuone` and NOT `fuzzy`; with autocomplete it
/// needs `preinsert` and NOT `fuzzy`, and is disabled outright while
/// `'ignorecase'` is on without `'infercase'`.
///
/// # Safety
/// Forwarded from [`get_cot_flags`].
#[must_use]
pub unsafe fn ins_compl_has_preinsert() -> bool {
    use crate::option_vars::opt_cot_flag::{FUZZY, MENUONE, PREINSERT};
    // SAFETY: forwarded from this function's own safety doc.
    let cur_cot_flags = unsafe { get_cot_flags() };
    // SAFETY: forwarded from this function's own safety doc.
    let autocomplete = unsafe { *COMPL_AUTOCOMPLETE.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };

    if autocomplete && opts.p_ic != 0 && opts.p_inf == 0 {
        return false;
    }
    if autocomplete {
        cur_cot_flags & (PREINSERT | FUZZY) == PREINSERT
    } else {
        cur_cot_flags & (PREINSERT | FUZZY | MENUONE) == (PREINSERT | MENUONE)
    }
}

/// The length of the completion so far (`get_compl_len`): from the
/// completion start column to the cursor column, never negative.
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer,
/// and this must not run concurrently with any write to `COMPL_COL`.
#[must_use]
pub unsafe fn get_compl_len() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let cursor_col = unsafe { (*crate::globals::GLOBALS.get_mut().curwin).w_cursor.col };
    // SAFETY: forwarded from this function's own safety doc.
    let start_col = unsafe { *COMPL_COL.get_mut() };
    (cursor_col - start_col).max(0)
}

/// The text typed so far that matches are filtered against
/// (`compl_leader`).
///
/// `None` is the original's NULL `.data`, meaning "no leader set" -
/// distinct from `Some(vec![])`, a leader that is set but empty. That
/// distinction is what [`ins_compl_leader`]'s fallback turns on, so
/// collapsing the two would change behaviour.
static COMPL_LEADER: GlobalCell<Option<Vec<u8>>> = GlobalCell::new(None);

/// The text as it was before completion started (`compl_orig_text`).
static COMPL_ORIG_TEXT: GlobalCell<Option<Vec<u8>>> = GlobalCell::new(None);

/// The current completion leader (`ins_compl_leader`), falling back to
/// the original text when no leader is set.
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_LEADER` or
/// `COMPL_ORIG_TEXT`.
#[must_use]
pub unsafe fn ins_compl_leader() -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let leader = unsafe { COMPL_LEADER.get_mut() };
    if let Some(leader) = leader.as_deref() {
        return Some(leader);
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { COMPL_ORIG_TEXT.get_mut() }.as_deref()
}

/// The length of the current completion leader
/// (`ins_compl_leader_len`), falling back to the original text's
/// length the same way [`ins_compl_leader`] does.
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_LEADER` or
/// `COMPL_ORIG_TEXT`.
#[must_use]
pub unsafe fn ins_compl_leader_len() -> usize {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ins_compl_leader() }.map_or(0, <[u8]>::len)
}

/// Whether the complete function returned `"always"` in the
/// `"refresh"` dictionary item (`compl_opt_refresh_always`).
static COMPL_OPT_REFRESH_ALWAYS: GlobalCell<bool> = GlobalCell::new(false);

/// Whether the previous attempt to find matches was interrupted
/// (`compl_was_interrupted`).
static COMPL_WAS_INTERRUPTED: GlobalCell<bool> = GlobalCell::new(false);

/// Whether the complete function asked to be re-run on every keystroke
/// (`ins_compl_refresh_always`).
///
/// Only meaningful for the function-driven completion modes, so the
/// flag alone is not enough.
///
/// # Safety
/// Forwarded from `ctrl_x_mode()`; must also not run concurrently with
/// any write to `COMPL_OPT_REFRESH_ALWAYS`.
#[must_use]
pub unsafe fn ins_compl_refresh_always() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let mode_uses_a_function =
        unsafe { ctrl_x_mode_function() } || unsafe { ctrl_x_mode_omni() };
    // SAFETY: forwarded from this function's own safety doc.
    mode_uses_a_function && unsafe { *COMPL_OPT_REFRESH_ALWAYS.get_mut() }
}

/// Whether matches must be looked up again, i.e. `ins_compl_restart`
/// should be called (`ins_compl_need_restart`).
///
/// True when the previous search did not finish, or when the complete
/// function asked to be refreshed every time.
///
/// # Safety
/// Forwarded from [`ins_compl_refresh_always`]; must also not run
/// concurrently with any write to `COMPL_WAS_INTERRUPTED`.
#[must_use]
pub unsafe fn ins_compl_need_restart() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *COMPL_WAS_INTERRUPTED.get_mut() || ins_compl_refresh_always() }
}

/// Whether the `'autocomplete'` option is on
/// (`ins_compl_has_autocomplete`).
///
/// Uses the buffer-local value when it is set, i.e. non-negative;
/// `-1` means "unset" and falls back to the global. Note this differs
/// from `'completeopt'`'s own convention, where `0` rather than a
/// negative value is the "unset" marker - see [`get_cot_flags`].
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer,
/// and this must not run concurrently with any write to
/// `crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn ins_compl_has_autocomplete() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let local = unsafe { (*crate::globals::GLOBALS.get_mut().curbuf).b_p_ac };
    if local >= 0 {
        return local != 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ac != 0
}

/// The index of the currently selected completion item
/// (`compl_selected_item`).
///
/// `-1` means "nothing selected", which is its own initial value in
/// the original.
static COMPL_SELECTED_ITEM: GlobalCell<i32> = GlobalCell::new(-1);

/// Decide the direction of Insert-mode completion from the key typed
/// (`ins_compl_key2dir`). Returns `Backward` or `Forward`.
///
/// For the externally-driven keys the direction follows whichever way
/// the requested item sits relative to the current selection, rather
/// than being fixed by the key itself.
///
/// # Safety
/// Must not run concurrently with any write to
/// `crate::popupmenu::PUM_WANT` or `COMPL_SELECTED_ITEM`.
#[must_use]
pub unsafe fn ins_compl_key2dir(c: i32) -> crate::vim_defs::Direction {
    use crate::keycodes_defs::{K_COMMAND, K_EVENT, K_LUA, K_PAGEUP, K_S_UP, K_UP};
    if c == K_EVENT || c == K_COMMAND || c == K_LUA {
        // SAFETY: forwarded from this function's own safety doc.
        let want = unsafe { *crate::popupmenu::PUM_WANT.get_mut() }.item;
        // SAFETY: forwarded from this function's own safety doc.
        let selected = unsafe { *COMPL_SELECTED_ITEM.get_mut() };
        return if want < selected {
            crate::vim_defs::Direction::Backward
        } else {
            crate::vim_defs::Direction::Forward
        };
    }
    if c == i32::from(crate::ascii_defs::CTRL_P)
        || c == i32::from(crate::ascii_defs::CTRL_L)
        || c == K_PAGEUP
        || c == crate::keycodes_defs::K_KPAGEUP
        || c == K_S_UP
        || c == K_UP
    {
        return crate::vim_defs::Direction::Backward;
    }
    crate::vim_defs::Direction::Forward
}

/// Whether `c` is a completion key that is only valid while the popup
/// menu is shown (`ins_compl_pum_key`).
#[must_use]
pub fn ins_compl_pum_key(c: i32) -> bool {
    use crate::keycodes_defs::{
        K_DOWN, K_KPAGEDOWN, K_KPAGEUP, K_PAGEDOWN, K_PAGEUP, K_S_DOWN, K_S_UP, K_UP,
    };
    crate::popupmenu::pum_visible()
        && (c == K_PAGEUP
            || c == K_KPAGEUP
            || c == K_S_UP
            || c == K_PAGEDOWN
            || c == K_KPAGEDOWN
            || c == K_S_DOWN
            || c == K_UP
            || c == K_DOWN)
}

/// Decide how many completions to move (`ins_compl_key2count`).
///
/// One for most keys; for the page-up/down keys the popup menu's own
/// height, less two lines of retained context when it is tall enough
/// for that to leave any movement.
///
/// # Safety
/// Must not run concurrently with any write to
/// `crate::popupmenu::PUM_WANT` or `COMPL_SELECTED_ITEM`.
#[must_use]
pub unsafe fn ins_compl_key2count(c: i32) -> i32 {
    use crate::keycodes_defs::{K_COMMAND, K_DOWN, K_EVENT, K_LUA, K_UP};
    if c == K_EVENT || c == K_COMMAND || c == K_LUA {
        // SAFETY: forwarded from this function's own safety doc.
        let want = unsafe { *crate::popupmenu::PUM_WANT.get_mut() }.item;
        // SAFETY: forwarded from this function's own safety doc.
        let selected = unsafe { *COMPL_SELECTED_ITEM.get_mut() };
        return want.saturating_sub(selected).saturating_abs();
    }

    if ins_compl_pum_key(c) && c != K_UP && c != K_DOWN {
        let mut h = crate::popupmenu::pum_get_height();
        if h > 3 {
            // Keep some context.
            h -= 2;
        }
        return h;
    }
    1
}

/// Find the start of the next word (`find_word_start`), as a byte
/// offset into `ptr`.
///
/// Skips over everything that is not word-ish, stopping at a NUL or a
/// newline. Returns an offset rather than the original's advanced
/// pointer, following this crate's established convention for
/// pointer-walking scans.
///
/// # Safety
/// Forwarded from [`crate::mbyte::mb_get_class`]/
/// [`crate::mbyte::utfc_ptr2len`].
#[must_use]
pub unsafe fn find_word_start(ptr: &[u8]) -> usize {
    let mut i = 0usize;
    loop {
        match ptr.get(i) {
            None | Some(&crate::ascii_defs::NUL) | Some(b'\n') => return i,
            Some(_) => {}
        }
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::mbyte::mb_get_class(&ptr[i..]) } > 1 {
            return i;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let len = unsafe { crate::mbyte::utfc_ptr2len(&ptr[i..]) };
        i += usize::try_from(len).unwrap_or(1).max(1);
    }
}

/// Find the end of the word (`find_word_end`), as a byte offset into
/// `ptr`. Assumes `ptr` starts inside a word.
///
/// Returns `0` when `ptr` does not start on a word character at all,
/// matching the original's `start_class > 1` guard - the scan is
/// skipped entirely rather than running to the end of the string.
///
/// # Safety
/// Forwarded from [`crate::mbyte::mb_get_class`]/
/// [`crate::mbyte::utfc_ptr2len`].
#[must_use]
pub unsafe fn find_word_end(ptr: &[u8]) -> usize {
    // SAFETY: forwarded from this function's own safety doc.
    let start_class = unsafe { crate::mbyte::mb_get_class(ptr) };
    if start_class <= 1 {
        return 0;
    }
    let mut i = 0usize;
    while !matches!(ptr.get(i), None | Some(&crate::ascii_defs::NUL)) {
        // SAFETY: forwarded from this function's own safety doc.
        let len = unsafe { crate::mbyte::utfc_ptr2len(&ptr[i..]) };
        i += usize::try_from(len).unwrap_or(1).max(1);
        if i >= ptr.len() {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::mbyte::mb_get_class(&ptr[i..]) } != start_class {
            break;
        }
    }
    i.min(ptr.len())
}

/// Find the end of the line (`find_line_end`), as a byte offset into
/// `ptr`, omitting any trailing CR and NL.
#[must_use]
pub fn find_line_end(ptr: &[u8]) -> usize {
    // The original scans to the NUL terminator, not to a Rust slice
    // length, so stop at the first NUL if there is one.
    let mut s = ptr.iter().position(|&c| c == crate::ascii_defs::NUL).unwrap_or(ptr.len());
    while s > 0 && matches!(ptr[s - 1], crate::ascii_defs::CAR | crate::ascii_defs::NL) {
        s -= 1;
    }
    s
}

/// Whether one of the matches was selected (`ins_compl_used_match`).
///
/// False when the match was edited instead, or when the longest
/// common string was used.
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_USED_MATCH`.
#[must_use]
pub unsafe fn ins_compl_used_match() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *COMPL_USED_MATCH.get_mut() }
}

/// Start over on finding the longest common string
/// (`ins_compl_init_get_longest`).
///
/// # Safety
/// Must not run concurrently with any other access to
/// `COMPL_GET_LONGEST`.
pub unsafe fn ins_compl_init_get_longest() {
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { COMPL_GET_LONGEST.get_mut() } = false;
}

/// Whether insert completion was interrupted
/// (`ins_compl_interrupted`).
///
/// Running out of the current source's time budget counts as an
/// interruption too, not only an explicit one.
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_INTERRUPTED` or
/// `COMPL_TIME_SLICE_EXPIRED`.
#[must_use]
pub unsafe fn ins_compl_interrupted() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *COMPL_INTERRUPTED.get_mut() || *COMPL_TIME_SLICE_EXPIRED.get_mut() }
}

/// Whether `<Enter>` selects a match in the completion popup menu
/// (`ins_compl_enter_selects`).
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_ENTER_SELECTS`.
#[must_use]
pub unsafe fn ins_compl_enter_selects() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *COMPL_ENTER_SELECTS.get_mut() }
}

/// The column where the text being completed starts (`ins_compl_col`).
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_COL`.
#[must_use]
pub unsafe fn ins_compl_col() -> crate::pos_defs::ColnrT {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *COMPL_COL.get_mut() }
}

/// The length in bytes of the text being completed (`ins_compl_len`).
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_LENGTH`.
#[must_use]
pub unsafe fn ins_compl_len() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *COMPL_LENGTH.get_mut() }
}

/// Whether autocompletion is active (`compl_autocomplete`).
///
/// Stays `false` today for the same reason as the statics above: the
/// completion engine that sets it is not translated.
static COMPL_AUTOCOMPLETE: GlobalCell<bool> = GlobalCell::new(false);

/// Whether the `'autocompletedelay'` timer is pending.
static COMPL_AUTOCOMPLETE_PENDING: GlobalCell<bool> = GlobalCell::new(false);
/// Nanosecond timestamp at which the delay was armed.
static COMPL_AUTOCOMPLETE_START_TV: GlobalCell<u64> = GlobalCell::new(0);

/// Arm the `'autocompletedelay'` timer when a delay is configured
/// (`ins_compl_arm_autocomplete_delay`).
///
/// # Safety
/// Reads `OPTION_VARS.p_acl` and mutates the completion delay statics.
#[must_use]
pub unsafe fn ins_compl_arm_autocomplete_delay() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_acl > 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            *COMPL_AUTOCOMPLETE_START_TV.get_mut() = crate::os::time::os_hrtime();
            *COMPL_AUTOCOMPLETE_PENDING.get_mut() = true;
        }
        true
    } else {
        false
    }
}

/// Get the local or global value of `'completeopt'` flags
/// (`get_cot_flags`).
///
/// A buffer-local value of `0` means "unset", so the global value is
/// used - the original spells this as a plain `!= 0` test rather than
/// tracking whether the local option was ever assigned.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer,
/// and this must not run concurrently with any write to
/// `crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn get_cot_flags() -> u32 {
    // SAFETY: forwarded from this function's own safety doc.
    let local = unsafe { (*crate::globals::GLOBALS.get_mut().curbuf).b_cot_flags };
    if local != 0 {
        return local;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cot_flags
}

/// Whether fuzzy matching is enabled (`cot_fuzzy`).
///
/// Thesaurus completion opts out, since its matches are looked up
/// rather than filtered.
///
/// # Safety
/// Forwarded from [`get_cot_flags`].
#[must_use]
pub unsafe fn cot_fuzzy() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { get_cot_flags() }) & crate::option_vars::opt_cot_flag::FUZZY != 0
        // SAFETY: forwarded from this function's own safety doc.
        && !unsafe { ctrl_x_mode_thesaurus() }
}

/// Whether matches should be sorted by proximity to the cursor
/// (`is_nearest_active`).
///
/// Fuzzy matching wins outright, since it imposes its own ordering.
///
/// # Safety
/// Forwarded from [`get_cot_flags`].
#[must_use]
pub unsafe fn is_nearest_active() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let autocomplete = unsafe { *COMPL_AUTOCOMPLETE.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let nearest = unsafe { get_cot_flags() } & crate::option_vars::opt_cot_flag::NEAREST != 0;
    // SAFETY: forwarded from this function's own safety doc.
    (autocomplete || nearest) && !unsafe { cot_fuzzy() }
}

/// Whether autocomplete is active and the pre-insert effect targets
/// the longest prefix (`ins_compl_preinsert_longest`).
///
/// The original masks `longest|preinsert|fuzzy` and compares the whole
/// result against `longest` alone, so this is true ONLY when
/// `'completeopt'` has `longest` WITHOUT either `preinsert` or
/// `fuzzy` - not merely when `longest` is present.
///
/// # Safety
/// Forwarded from [`get_cot_flags`].
#[must_use]
pub unsafe fn ins_compl_preinsert_longest() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { *COMPL_AUTOCOMPLETE.get_mut() } {
        return false;
    }
    use crate::option_vars::opt_cot_flag::{FUZZY, LONGEST, PREINSERT};
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { get_cot_flags() }) & (LONGEST | PREINSERT | FUZZY) == LONGEST
}

/// "normal" or "adding" expansion (`CONT_ADDING`).
pub const CONT_ADDING: i32 = 1;
/// A `^X` interrupted the current expansion (`CONT_INTRPT`).
///
/// Deliberately `2 + 4` in the original: it implies
/// [`CONT_N_ADDS`], so testing for it also reports "next `^X<>` will
/// add-new or expand-current".
pub const CONT_INTRPT: i32 = 2 + 4;
/// Next `^X<>` will add-new or expand-current (`CONT_N_ADDS`).
pub const CONT_N_ADDS: i32 = 4;
/// Next `^X<>` will set the initial position (`CONT_S_IPOS`).
pub const CONT_S_IPOS: i32 = 8;
/// Pattern includes start of line, just for word-wise expansion
/// (`CONT_SOL`).
pub const CONT_SOL: i32 = 16;
/// For `ctrl_x_mode` 0, `^X^P`/`^X^N` do a local completion
/// (`CONT_LOCAL`).
pub const CONT_LOCAL: i32 = 32;

/// Flags tracking how the current completion continues
/// (`compl_cont_status`).
///
/// Stays `0` in this crate today for the same reason as
/// [`CTRL_X_MODE`]/[`COMPL_STARTED`]: nothing translated yet can start
/// a real completion session to set it.
static COMPL_CONT_STATUS: GlobalCell<i32> = GlobalCell::new(0);

/// Direction the completion is searching in (`compl_direction`).
static COMPL_DIRECTION: GlobalCell<crate::vim_defs::Direction> =
    GlobalCell::new(crate::vim_defs::Direction::Forward);

/// Direction whose matches are currently being shown
/// (`compl_shows_dir`).
///
/// Tracked separately from [`COMPL_DIRECTION`] because the displayed
/// direction can differ from the one being searched.
static COMPL_SHOWS_DIR: GlobalCell<crate::vim_defs::Direction> =
    GlobalCell::new(crate::vim_defs::Direction::Forward);

/// Whether in "normal" or "adding" insert completion matches state
/// (`compl_status_adding`).
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_CONT_STATUS`.
#[must_use]
pub unsafe fn compl_status_adding() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { *COMPL_CONT_STATUS.get_mut() }) & CONT_ADDING != 0
}

/// Whether the completion pattern includes the start of the line, just
/// for word-wise expansion (`compl_status_sol`).
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_CONT_STATUS`.
#[must_use]
pub unsafe fn compl_status_sol() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { *COMPL_CONT_STATUS.get_mut() }) & CONT_SOL != 0
}

/// Whether `^X^P`/`^X^N` will do a local completion, i.e. use
/// `complete=.` (`compl_status_local`).
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_CONT_STATUS`.
#[must_use]
pub unsafe fn compl_status_local() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { *COMPL_CONT_STATUS.get_mut() }) & CONT_LOCAL != 0
}

/// Clear the completion status flags (`compl_status_clear`).
///
/// # Safety
/// Must not run concurrently with any other access to
/// `COMPL_CONT_STATUS`.
pub unsafe fn compl_status_clear() {
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { COMPL_CONT_STATUS.get_mut() } = 0;
}

/// Whether completion is using the forward direction matches
/// (`compl_dir_forward`).
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_DIRECTION`.
#[must_use]
pub unsafe fn compl_dir_forward() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { *COMPL_DIRECTION.get_mut() }) == crate::vim_defs::Direction::Forward
}

/// Whether forward completion matches are currently being shown
/// (`compl_shows_dir_forward`).
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_SHOWS_DIR`.
#[must_use]
pub unsafe fn compl_shows_dir_forward() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { *COMPL_SHOWS_DIR.get_mut() }) == crate::vim_defs::Direction::Forward
}

/// Whether backward completion matches are currently being shown
/// (`compl_shows_dir_backward`).
///
/// Note this is NOT the negation of [`compl_shows_dir_forward`]: the
/// original's `Direction` also has the `FORWARD_FILE`/`BACKWARD_FILE`
/// values, so both can be false at once.
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_SHOWS_DIR`.
#[must_use]
pub unsafe fn compl_shows_dir_backward() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { *COMPL_SHOWS_DIR.get_mut() }) == crate::vim_defs::Direction::Backward
}

/// Check that Insert-mode completion is active (`ins_compl_active`).
///
/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ins_compl_active() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *COMPL_STARTED.get_mut() }
}

/// # Safety
/// Single-threaded test/editor state, matching every other
/// `GlobalCell`-backed file-static in this crate.
unsafe fn ctrl_x_mode() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *CTRL_X_MODE.get_mut() }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_none() -> bool {
    unsafe { ctrl_x_mode() == 0 }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_normal() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_NORMAL }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_scroll() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_SCROLL }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_whole_line() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_WHOLE_LINE }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_files() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_FILES }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_tags() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_TAGS }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_path_patterns() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_PATH_PATTERNS }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_path_defines() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_PATH_DEFINES }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_dictionary() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_DICTIONARY }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_thesaurus() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_THESAURUS }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_cmdline() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_CMDLINE || ctrl_x_mode() == CTRL_X_CMDLINE_CTRL_X }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_function() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_FUNCTION }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_omni() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_OMNI }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_spell() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_SPELL }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_eval() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_EVAL }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_line_or_eval() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_WHOLE_LINE || ctrl_x_mode() == CTRL_X_EVAL }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_register() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_REGISTER }
}

/// Whether other than default completion has been selected.
///
/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_not_default() -> bool {
    unsafe { ctrl_x_mode() != CTRL_X_NORMAL }
}

/// Whether CTRL-X was typed without a following character, not
/// including when in `CTRL_X_CMDLINE_CTRL_X` mode.
///
/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_not_defined_yet() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_NOT_DEFINED_YET }
}

/// The `'completefunc'` callback (`cfu_cb`, a file-static `Callback`).
/// Nothing in this crate can currently set a real value here - see
/// `ops.rs`'s `OPFUNC_CB` for the identical reasoning (needs
/// `option_set_callback_func`, not translated).
static CFU_CB: GlobalCell<crate::eval::typval_defs::Callback> =
    GlobalCell::new(crate::eval::typval_defs::Callback::None);

/// The `'omnifunc'` callback (`ofu_cb`). See [`CFU_CB`].
static OFU_CB: GlobalCell<crate::eval::typval_defs::Callback> =
    GlobalCell::new(crate::eval::typval_defs::Callback::None);

/// The `'thesaurusfunc'` callback (`tsrfu_cb`). See [`CFU_CB`].
static TSRFU_CB: GlobalCell<crate::eval::typval_defs::Callback> =
    GlobalCell::new(crate::eval::typval_defs::Callback::None);

/// The `'completefunc'`-style callbacks associated with each `F{func}`
/// entry in `'complete'`/`'completeopt'` (`cpt_cb`/`cpt_cb_count`
/// collapsed into one `Vec`, matching this crate's established
/// "translate a `T*`+count pair as a `Vec<T>` when nothing needs the
/// original's raw-pointer/manual-count shape" precedent - e.g.
/// `runtime.rs`'s own `SCRIPT_ITEMS`). Always empty today: nothing in
/// this crate can currently populate it.
static CPT_CB: GlobalCell<Vec<crate::eval::typval_defs::Callback>> = GlobalCell::new(Vec::new());

/// Mark `copy_id` on every callback in `callbacks` so none of them are
/// garbage collected (`set_ref_in_cpt_callbacks`).
///
/// Uses `||`'s own short-circuit evaluation (matching the original's
/// `abort = abort || set_ref_in_callback(...)`): once `abort` becomes
/// `true`, later callbacks in `callbacks` are NOT visited at all for
/// this call - a faithful translation of the original's real
/// structure, even though nothing in this crate can currently make
/// `abort` become `true` at all (every callback here is always
/// [`crate::eval::typval_defs::Callback::None`] today).
///
/// # Safety
/// Same as [`crate::eval::eval::set_ref_in_callback`].
pub unsafe fn set_ref_in_cpt_callbacks(
    callbacks: &[crate::eval::typval_defs::Callback],
    copy_id: i32,
) -> bool {
    let mut abort = false;
    for cb in callbacks {
        abort = abort
            // SAFETY: forwarded from this function's own safety doc.
            || unsafe {
                crate::eval::eval::set_ref_in_callback(cb, copy_id, std::ptr::null_mut(), std::ptr::null_mut())
            };
    }
    abort
}

/// Mark the global `'completefunc'`/`'omnifunc'`/`'thesaurusfunc'`
/// callbacks, plus every `F{func}` callback in `'complete'`, with
/// `copy_id` so none of them are garbage collected
/// (`set_ref_in_insexpand_funcs`).
///
/// # Safety
/// Same as [`crate::eval::eval::set_ref_in_callback`].
pub unsafe fn set_ref_in_insexpand_funcs(copy_id: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let cfu = unsafe { &*CFU_CB.as_ptr() };
    // SAFETY: forwarded from this function's own safety doc.
    let ofu = unsafe { &*OFU_CB.as_ptr() };
    // SAFETY: forwarded from this function's own safety doc.
    let tsrfu = unsafe { &*TSRFU_CB.as_ptr() };
    // SAFETY: forwarded from this function's own safety doc.
    let cpt = unsafe { &*CPT_CB.as_ptr() };

    // SAFETY: forwarded from this function's own safety doc.
    let mut abort = unsafe {
        crate::eval::eval::set_ref_in_callback(cfu, copy_id, std::ptr::null_mut(), std::ptr::null_mut())
    };
    abort = abort
        // SAFETY: forwarded from this function's own safety doc.
        || unsafe {
            crate::eval::eval::set_ref_in_callback(ofu, copy_id, std::ptr::null_mut(), std::ptr::null_mut())
        };
    abort = abort
        // SAFETY: forwarded from this function's own safety doc.
        || unsafe {
            crate::eval::eval::set_ref_in_callback(tsrfu, copy_id, std::ptr::null_mut(), std::ptr::null_mut())
        };
    abort = abort || unsafe { set_ref_in_cpt_callbacks(cpt, copy_id) };

    abort
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::global_state_test_lock;

    // ---- compl_shown_match predicates ----

    /// Installs a shown match and restores the previous pointer on
    /// drop, even through a panic, so a failing test cannot leave a
    /// dangling pointer in the global for the next test.
    struct ShownMatchGuard {
        saved: *mut ComplT,
        saved_col: crate::pos_defs::ColnrT,
        saved_curwin: *mut crate::buffer_defs::WinT,
    }

    impl ShownMatchGuard {
        fn install(m: *mut ComplT) -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let me = Self {
                saved: unsafe { *COMPL_SHOWN_MATCH.get_mut() },
                saved_col: unsafe { *COMPL_COL.get_mut() },
                saved_curwin: g.curwin,
            };
            unsafe { *COMPL_SHOWN_MATCH.get_mut() = m };
            me
        }
    }

    impl Drop for ShownMatchGuard {
        fn drop(&mut self) {
            unsafe { *COMPL_SHOWN_MATCH.get_mut() = self.saved };
            unsafe { *COMPL_COL.get_mut() = self.saved_col };
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin = self.saved_curwin;
        }
    }

    fn with_text(text: &[u8]) -> Box<ComplT> {
        Box::new(ComplT { cp_str: Some(text.to_vec()), ..ComplT::default() })
    }

    /// A multi-line match is one string containing newlines, not a
    /// separate count.
    #[test]
    fn ins_compl_has_multiple_detects_a_newline_in_the_shown_match() {
        let _lock = global_state_test_lock();
        let mut single = with_text(b"one line");
        let _g = ShownMatchGuard::install(std::ptr::from_mut(&mut *single));
        assert!(!unsafe { ins_compl_has_multiple() });

        let mut multi = with_text(b"first\nsecond");
        unsafe { *COMPL_SHOWN_MATCH.get_mut() = std::ptr::from_mut(&mut *multi) };
        assert!(unsafe { ins_compl_has_multiple() });
    }

    /// With no match shown there is nothing to be stuck on, so this
    /// reports true.
    #[test]
    fn ins_compl_has_shown_match_is_true_with_no_match_at_all() {
        let _lock = global_state_test_lock();
        let _g = ShownMatchGuard::install(std::ptr::null_mut());
        assert!(unsafe { ins_compl_has_shown_match() });
    }

    /// A LONE match links to itself in the circular list; that
    /// self-link is exactly what marks "there is nowhere else to go".
    /// Comparing against null instead would wrongly report true here.
    #[test]
    fn ins_compl_has_shown_match_is_false_for_a_single_self_linked_match() {
        let _lock = global_state_test_lock();
        let mut only = with_text(b"only");
        let ptr = std::ptr::from_mut(&mut *only);
        only.cp_next = ptr;
        let _g = ShownMatchGuard::install(ptr);
        assert!(!unsafe { ins_compl_has_shown_match() });
    }

    #[test]
    fn ins_compl_has_shown_match_is_true_when_another_match_follows() {
        let _lock = global_state_test_lock();
        let mut second = with_text(b"second");
        let mut first = with_text(b"first");
        first.cp_next = std::ptr::from_mut(&mut *second);
        let _g = ShownMatchGuard::install(std::ptr::from_mut(&mut *first));
        assert!(unsafe { ins_compl_has_shown_match() });
    }

    #[test]
    fn ins_compl_long_shown_match_is_false_without_a_match() {
        let _lock = global_state_test_lock();
        let _g = ShownMatchGuard::install(std::ptr::null_mut());
        assert!(!unsafe { ins_compl_long_shown_match() });
    }

    /// The comparison is against how much has been TYPED
    /// (cursor column minus the completion start), not against the
    /// cursor column alone.
    #[test]
    fn ins_compl_long_shown_match_compares_against_the_typed_length() {
        let _lock = global_state_test_lock();
        let mut m = with_text(b"abcde"); // 5 bytes
        let mut win = Box::new(crate::buffer_defs::WinT::default());
        let _g = ShownMatchGuard::install(std::ptr::from_mut(&mut *m));

        win.w_cursor.col = 13;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = std::ptr::from_mut(&mut *win);
        // Completion starts at column 10, so 3 bytes are typed; a
        // 5-byte match is longer than that.
        unsafe { *COMPL_COL.get_mut() = 10 };
        assert!(unsafe { ins_compl_long_shown_match() });

        // Now 7 bytes are typed, which the 5-byte match cannot cover.
        unsafe { *COMPL_COL.get_mut() = 6 };
        assert!(!unsafe { ins_compl_long_shown_match() });
    }

    // ---- ComplT accessors and comparators ----

    fn scored(score: i32) -> ComplT {
        ComplT { cp_score: score, ..ComplT::default() }
    }

    #[test]
    fn match_at_original_text_reads_only_its_own_flag() {
        let plain = ComplT::default();
        assert!(!match_at_original_text(&plain));

        let original = ComplT { cp_flags: cp_flags::ORIGINAL_TEXT, ..ComplT::default() };
        assert!(match_at_original_text(&original));

        // Another flag set on its own must not be mistaken for it.
        let other = ComplT { cp_flags: cp_flags::ICASE | cp_flags::FAST, ..ComplT::default() };
        assert!(!match_at_original_text(&other));

        // ...and it is still detected alongside other flags.
        let both = ComplT {
            cp_flags: cp_flags::ORIGINAL_TEXT | cp_flags::ICASE,
            ..ComplT::default()
        };
        assert!(match_at_original_text(&both));
    }

    #[test]
    fn cp_link_accessors_round_trip() {
        let mut a = ComplT::default();
        let mut b = ComplT::default();
        let b_ptr = std::ptr::from_mut(&mut b);

        assert!(cp_get_next(&a).is_null());
        assert!(cp_get_prev(&a).is_null());

        cp_set_next(&mut a, b_ptr);
        cp_set_prev(&mut a, b_ptr);
        assert_eq!(cp_get_next(&a), b_ptr);
        assert_eq!(cp_get_prev(&a), b_ptr);
    }

    /// Fuzzy ordering is DESCENDING: the best match sorts first.
    #[test]
    fn cp_compare_fuzzy_puts_the_higher_score_first() {
        let high = scored(100);
        let low = scored(10);
        assert!(cp_compare_fuzzy(&high, &low) < 0);
        assert!(cp_compare_fuzzy(&low, &high) > 0);
        assert_eq!(cp_compare_fuzzy(&scored(5), &scored(5)), 0);
    }

    /// Proximity ordering is ASCENDING - the exact OPPOSITE of the
    /// fuzzy comparator. Sharing one direction between the two would
    /// break one of them, so this asserts they genuinely disagree on
    /// the same pair.
    #[test]
    fn cp_compare_nearest_puts_the_lower_score_first() {
        let near = scored(1);
        let far = scored(50);
        assert!(cp_compare_nearest(&near, &far) < 0);
        assert!(cp_compare_nearest(&far, &near) > 0);
        assert_eq!(cp_compare_nearest(&scored(7), &scored(7)), 0);

        // The two comparators disagree on the same pair, by design.
        assert!(
            cp_compare_nearest(&near, &far).signum() != cp_compare_fuzzy(&near, &far).signum()
        );
    }

    /// An unscored match compares EQUAL to everything, leaving it
    /// where it is rather than sorting it to one end. Since
    /// `FUZZY_SCORE_NONE` is `i32::MIN`, a comparator missing this
    /// check would sort such entries to the very front.
    #[test]
    fn cp_compare_nearest_treats_an_unscored_match_as_equal() {
        let none = scored(crate::fuzzy::FUZZY_SCORE_NONE);
        let real = scored(50);
        assert_eq!(cp_compare_nearest(&none, &real), 0);
        assert_eq!(cp_compare_nearest(&real, &none), 0);
        assert_eq!(cp_compare_nearest(&none, &none), 0);
    }

    #[test]
    fn cp_compare_fuzzy_sorts_a_list_best_first() {
        let mut items = [scored(10), scored(90), scored(50)];
        items.sort_by(|a, b| cp_compare_fuzzy(a, b).cmp(&0));
        let scores: Vec<i32> = items.iter().map(|m| m.cp_score).collect();
        assert_eq!(scores, vec![90, 50, 10]);
    }

    #[test]
    fn cp_compare_nearest_sorts_a_list_closest_first() {
        let mut items = [scored(10), scored(90), scored(50)];
        items.sort_by(|a, b| cp_compare_nearest(a, b).cmp(&0));
        let scores: Vec<i32> = items.iter().map(|m| m.cp_score).collect();
        assert_eq!(scores, vec![10, 50, 90]);
    }

    /// RAII guard temporarily overriding `CTRL_X_MODE`, restoring the
    /// previous value on drop (even on test panic).
    struct CtrlXModeGuard {
        prev: i32,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CtrlXModeGuard {
        fn set(value: i32) -> Self {
            let _lock = global_state_test_lock();
            let prev = unsafe { *CTRL_X_MODE.get_mut() };
            unsafe { *CTRL_X_MODE.get_mut() = value };
            Self { prev, _lock }
        }
    }

    impl Drop for CtrlXModeGuard {
        fn drop(&mut self) {
            unsafe { *CTRL_X_MODE.get_mut() = self.prev };
        }
    }

    /// RAII guard temporarily overriding the completion continuation
    /// state, restoring the previous values on drop (even on panic).
    struct ComplStateGuard {
        prev_status: i32,
        prev_dir: crate::vim_defs::Direction,
        prev_shows: crate::vim_defs::Direction,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    struct AutocompleteDelayGuard {
        prev_delay: crate::types_defs::OptInt,
        prev_pending: bool,
        prev_start: u64,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl AutocompleteDelayGuard {
        fn set(delay: crate::types_defs::OptInt) -> Self {
            let _lock = global_state_test_lock();
            let prev_delay = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_acl;
            let prev_pending = unsafe { *COMPL_AUTOCOMPLETE_PENDING.get_mut() };
            let prev_start = unsafe { *COMPL_AUTOCOMPLETE_START_TV.get_mut() };
            unsafe {
                crate::option_vars::OPTION_VARS.get_mut().p_acl = delay;
                *COMPL_AUTOCOMPLETE_PENDING.get_mut() = false;
                *COMPL_AUTOCOMPLETE_START_TV.get_mut() = 0;
            }
            Self { prev_delay, prev_pending, prev_start, _lock }
        }
    }

    impl Drop for AutocompleteDelayGuard {
        fn drop(&mut self) {
            unsafe {
                crate::option_vars::OPTION_VARS.get_mut().p_acl = self.prev_delay;
                *COMPL_AUTOCOMPLETE_PENDING.get_mut() = self.prev_pending;
                *COMPL_AUTOCOMPLETE_START_TV.get_mut() = self.prev_start;
            }
        }
    }

    impl ComplStateGuard {
        fn new() -> Self {
            let _lock = global_state_test_lock();
            Self {
                prev_status: unsafe { *COMPL_CONT_STATUS.get_mut() },
                prev_dir: unsafe { *COMPL_DIRECTION.get_mut() },
                prev_shows: unsafe { *COMPL_SHOWS_DIR.get_mut() },
                _lock,
            }
        }
    }

    impl Drop for ComplStateGuard {
        fn drop(&mut self) {
            unsafe {
                *COMPL_CONT_STATUS.get_mut() = self.prev_status;
                *COMPL_DIRECTION.get_mut() = self.prev_dir;
                *COMPL_SHOWS_DIR.get_mut() = self.prev_shows;
            }
        }
    }

    #[test]
    fn autocomplete_delay_is_only_armed_for_a_positive_option_value() {
        {
            let _guard = AutocompleteDelayGuard::set(0);
            assert!(!unsafe { ins_compl_arm_autocomplete_delay() });
            assert!(!unsafe { *COMPL_AUTOCOMPLETE_PENDING.get_mut() });
            assert_eq!(unsafe { *COMPL_AUTOCOMPLETE_START_TV.get_mut() }, 0);
        }

        let _guard = AutocompleteDelayGuard::set(50);
        let before = crate::os::time::os_hrtime();
        assert!(unsafe { ins_compl_arm_autocomplete_delay() });
        let after = crate::os::time::os_hrtime();
        let start = unsafe { *COMPL_AUTOCOMPLETE_START_TV.get_mut() };
        assert!(unsafe { *COMPL_AUTOCOMPLETE_PENDING.get_mut() });
        assert!((before..=after).contains(&start));
    }

    /// Installs a buffer as `curbuf` for the test's duration and
    /// restores the previous one on drop, so a failing assertion
    /// cannot leave a dangling pointer behind.
    struct CurbufGuard {
        prev: *mut crate::buffer_defs::BufT,
    }

    impl CurbufGuard {
        fn set(buf: &mut crate::buffer_defs::BufT) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let prev = globals.curbuf;
            globals.curbuf = buf as *mut crate::buffer_defs::BufT;
            Self { prev }
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.prev;
        }
    }

    /// Saves and restores the global `'completeopt'` flags.
    struct CotFlagsGuard {
        saved: u32,
    }

    impl CotFlagsGuard {
        fn set(flags: u32) -> Self {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let saved = opts.cot_flags;
            opts.cot_flags = flags;
            Self { saved }
        }
    }

    impl Drop for CotFlagsGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cot_flags = self.saved;
        }
    }

    #[test]
    fn pum_wanted_needs_menu_or_menuone_or_autocomplete() {
        let _lock = global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);
        use crate::option_vars::opt_cot_flag::{FUZZY, MENU, MENUONE};

        for (flags, expected) in [(MENU, true), (MENUONE, true), (FUZZY, false), (0, false)] {
            let _cot = CotFlagsGuard::set(flags);
            assert_eq!(unsafe { pum_wanted() }, expected, "flags {flags:#x}");
        }
    }

    #[test]
    fn pum_wanted_is_forced_on_by_autocomplete() {
        let _lock = global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);
        let _cot = CotFlagsGuard::set(0);

        unsafe { *COMPL_AUTOCOMPLETE.get_mut() = true };
        assert!(unsafe { pum_wanted() });
        unsafe { *COMPL_AUTOCOMPLETE.get_mut() = false };
    }

    #[test]
    fn ins_compl_has_preinsert_without_autocomplete_needs_menuone_too() {
        let _lock = global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);
        use crate::option_vars::opt_cot_flag::{FUZZY, MENUONE, PREINSERT};

        for (flags, expected) in [
            // preinsert alone is NOT enough here.
            (PREINSERT, false),
            (PREINSERT | MENUONE, true),
            // fuzzy disables it either way.
            (PREINSERT | MENUONE | FUZZY, false),
            (MENUONE, false),
        ] {
            let _cot = CotFlagsGuard::set(flags);
            assert_eq!(
                unsafe { ins_compl_has_preinsert() },
                expected,
                "flags {flags:#x}"
            );
        }
    }

    #[test]
    fn ins_compl_has_preinsert_with_autocomplete_drops_the_menuone_requirement() {
        let _lock = global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);
        use crate::option_vars::opt_cot_flag::{FUZZY, PREINSERT};

        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let (pic, pinf) = (opts.p_ic, opts.p_inf);
        opts.p_ic = 0;
        unsafe { *COMPL_AUTOCOMPLETE.get_mut() = true };

        {
            let _cot = CotFlagsGuard::set(PREINSERT);
            assert!(unsafe { ins_compl_has_preinsert() });
        }
        {
            let _cot = CotFlagsGuard::set(PREINSERT | FUZZY);
            assert!(!unsafe { ins_compl_has_preinsert() });
        }

        unsafe { *COMPL_AUTOCOMPLETE.get_mut() = false };
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        (opts.p_ic, opts.p_inf) = (pic, pinf);
    }

    #[test]
    fn ins_compl_has_preinsert_is_off_for_ignorecase_without_infercase() {
        let _lock = global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);
        let _cot = CotFlagsGuard::set(crate::option_vars::opt_cot_flag::PREINSERT);

        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let (pic, pinf) = (opts.p_ic, opts.p_inf);
        unsafe { *COMPL_AUTOCOMPLETE.get_mut() = true };

        // This early-out applies only with autocomplete on.
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        (opts.p_ic, opts.p_inf) = (1, 0);
        assert!(!unsafe { ins_compl_has_preinsert() });

        // 'infercase' cancels the early-out.
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_inf = 1;
        assert!(unsafe { ins_compl_has_preinsert() });

        unsafe { *COMPL_AUTOCOMPLETE.get_mut() = false };
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        (opts.p_ic, opts.p_inf) = (pic, pinf);
    }

    #[test]
    fn get_compl_len_is_the_distance_from_the_start_column() {
        let _lock = global_state_test_lock();
        let mut win = crate::buffer_defs::WinT {
            w_cursor: crate::pos_defs::PosT { lnum: 1, col: 10, coladd: 0 },
            ..Default::default()
        };
        // Guarded: `win` is a local, so an assertion failure below
        // would otherwise leave `curwin` dangling for the next test.
        let _cw = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.curwin,
                &mut win as *mut crate::buffer_defs::WinT,
            )
        };
        let prev_col = unsafe { *COMPL_COL.get_mut() };

        unsafe { *COMPL_COL.get_mut() = 4 };
        assert_eq!(unsafe { get_compl_len() }, 6);

        // Never negative, even when the cursor sits before the start.
        unsafe { *COMPL_COL.get_mut() = 20 };
        assert_eq!(unsafe { get_compl_len() }, 0);

        unsafe { *COMPL_COL.get_mut() = prev_col };
    }

    /// Saves and restores the completion leader/original-text pair.
    struct ComplTextGuard {
        leader: Option<Vec<u8>>,
        orig: Option<Vec<u8>>,
    }

    impl ComplTextGuard {
        fn new() -> Self {
            Self {
                leader: unsafe { COMPL_LEADER.get_mut() }.clone(),
                orig: unsafe { COMPL_ORIG_TEXT.get_mut() }.clone(),
            }
        }
    }

    impl Drop for ComplTextGuard {
        fn drop(&mut self) {
            unsafe {
                *COMPL_LEADER.get_mut() = self.leader.take();
                *COMPL_ORIG_TEXT.get_mut() = self.orig.take();
            }
        }
    }

    #[test]
    fn ins_compl_leader_prefers_the_leader_over_the_original_text() {
        let _lock = global_state_test_lock();
        let _guard = ComplTextGuard::new();

        unsafe {
            *COMPL_LEADER.get_mut() = Some(b"lead".to_vec());
            *COMPL_ORIG_TEXT.get_mut() = Some(b"original".to_vec());
        }
        assert_eq!(unsafe { ins_compl_leader() }, Some(&b"lead"[..]));
        assert_eq!(unsafe { ins_compl_leader_len() }, 4);
    }

    #[test]
    fn ins_compl_leader_falls_back_to_the_original_text() {
        let _lock = global_state_test_lock();
        let _guard = ComplTextGuard::new();

        unsafe {
            *COMPL_LEADER.get_mut() = None;
            *COMPL_ORIG_TEXT.get_mut() = Some(b"original".to_vec());
        }
        assert_eq!(unsafe { ins_compl_leader() }, Some(&b"original"[..]));
        assert_eq!(unsafe { ins_compl_leader_len() }, 8);
    }

    #[test]
    fn ins_compl_leader_distinguishes_an_empty_leader_from_no_leader() {
        let _lock = global_state_test_lock();
        let _guard = ComplTextGuard::new();

        // The original tests `.data != NULL`, not the size, so a
        // leader that is SET BUT EMPTY still wins over the original
        // text. Collapsing None and Some(vec![]) would break this.
        unsafe {
            *COMPL_LEADER.get_mut() = Some(Vec::new());
            *COMPL_ORIG_TEXT.get_mut() = Some(b"original".to_vec());
        }
        assert_eq!(unsafe { ins_compl_leader() }, Some(&b""[..]));
        assert_eq!(unsafe { ins_compl_leader_len() }, 0);
    }

    #[test]
    fn ins_compl_leader_is_none_when_neither_is_set() {
        let _lock = global_state_test_lock();
        let _guard = ComplTextGuard::new();

        unsafe {
            *COMPL_LEADER.get_mut() = None;
            *COMPL_ORIG_TEXT.get_mut() = None;
        }
        assert_eq!(unsafe { ins_compl_leader() }, None);
        assert_eq!(unsafe { ins_compl_leader_len() }, 0);
    }

    #[test]
    fn ins_compl_refresh_always_needs_a_function_driven_mode() {
        // NOTE: no global_state_test_lock() here - CtrlXModeGuard::set
        // takes it internally, and the lock is not reentrant.
        let prev = unsafe { *COMPL_OPT_REFRESH_ALWAYS.get_mut() };
        unsafe { *COMPL_OPT_REFRESH_ALWAYS.get_mut() = true };

        // The flag alone is not enough outside the function modes.
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_NORMAL);
            assert!(!unsafe { ins_compl_refresh_always() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_FUNCTION);
            assert!(unsafe { ins_compl_refresh_always() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_OMNI);
            assert!(unsafe { ins_compl_refresh_always() });
        }

        unsafe { *COMPL_OPT_REFRESH_ALWAYS.get_mut() = prev };
    }

    #[test]
    fn ins_compl_refresh_always_needs_the_flag_too() {
        let _guard = CtrlXModeGuard::set(CTRL_X_FUNCTION);
        let prev = unsafe { *COMPL_OPT_REFRESH_ALWAYS.get_mut() };
        unsafe { *COMPL_OPT_REFRESH_ALWAYS.get_mut() = false };

        assert!(!unsafe { ins_compl_refresh_always() });

        unsafe { *COMPL_OPT_REFRESH_ALWAYS.get_mut() = prev };
    }

    #[test]
    fn ins_compl_need_restart_covers_both_causes() {
        let _lock = global_state_test_lock();
        let (pw, pr) = unsafe {
            (*COMPL_WAS_INTERRUPTED.get_mut(), *COMPL_OPT_REFRESH_ALWAYS.get_mut())
        };

        unsafe {
            *COMPL_WAS_INTERRUPTED.get_mut() = false;
            *COMPL_OPT_REFRESH_ALWAYS.get_mut() = false;
        }
        assert!(!unsafe { ins_compl_need_restart() });

        // An unfinished previous search alone forces a restart, with
        // no function mode involved.
        unsafe { *COMPL_WAS_INTERRUPTED.get_mut() = true };
        assert!(unsafe { ins_compl_need_restart() });

        unsafe {
            *COMPL_WAS_INTERRUPTED.get_mut() = pw;
            *COMPL_OPT_REFRESH_ALWAYS.get_mut() = pr;
        }
    }

    #[test]
    fn ins_compl_has_autocomplete_prefers_a_nonnegative_local_value() {
        let _lock = global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev_global = opts.p_ac;
        opts.p_ac = 1;

        // A local 0 is a real "off", not "unset" - unlike
        // 'completeopt', where 0 means unset.
        let mut buf = crate::buffer_defs::BufT { b_p_ac: 0, ..Default::default() };
        {
            let _curbuf = CurbufGuard::set(&mut buf);
            assert!(!unsafe { ins_compl_has_autocomplete() });
        }

        let mut buf = crate::buffer_defs::BufT { b_p_ac: 1, ..Default::default() };
        {
            let _curbuf = CurbufGuard::set(&mut buf);
            assert!(unsafe { ins_compl_has_autocomplete() });
        }

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ac = prev_global;
    }

    #[test]
    fn ins_compl_has_autocomplete_falls_back_on_a_negative_local_value() {
        let _lock = global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev_global = opts.p_ac;

        // -1 is the "unset" marker, so the global decides.
        let mut buf = crate::buffer_defs::BufT { b_p_ac: -1, ..Default::default() };
        let _curbuf = CurbufGuard::set(&mut buf);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ac = 1;
        assert!(unsafe { ins_compl_has_autocomplete() });

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ac = 0;
        assert!(!unsafe { ins_compl_has_autocomplete() });

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ac = prev_global;
    }

    #[test]
    fn ins_compl_key2dir_maps_backward_keys() {
        let _lock = global_state_test_lock();
        use crate::keycodes_defs::{K_KPAGEUP, K_PAGEUP, K_S_UP, K_UP};
        for c in [
            i32::from(crate::ascii_defs::CTRL_P),
            i32::from(crate::ascii_defs::CTRL_L),
            K_PAGEUP,
            K_KPAGEUP,
            K_S_UP,
            K_UP,
        ] {
            assert_eq!(
                unsafe { ins_compl_key2dir(c) },
                crate::vim_defs::Direction::Backward,
                "key {c}"
            );
        }
    }

    #[test]
    fn ins_compl_key2dir_defaults_to_forward() {
        let _lock = global_state_test_lock();
        use crate::keycodes_defs::{K_DOWN, K_PAGEDOWN};
        for c in [
            i32::from(crate::ascii_defs::CTRL_N),
            K_PAGEDOWN,
            K_DOWN,
            i32::from(b'x'),
        ] {
            assert_eq!(
                unsafe { ins_compl_key2dir(c) },
                crate::vim_defs::Direction::Forward,
                "key {c}"
            );
        }
    }

    #[test]
    fn ins_compl_key2dir_follows_the_requested_item_for_external_keys() {
        let _lock = global_state_test_lock();
        let prev_want = unsafe { *crate::popupmenu::PUM_WANT.get_mut() };
        let prev_sel = unsafe { *COMPL_SELECTED_ITEM.get_mut() };
        unsafe { *COMPL_SELECTED_ITEM.get_mut() = 5 };

        // The direction is not fixed by the key: it follows whichever
        // way the requested item sits from the current selection.
        unsafe { crate::popupmenu::PUM_WANT.get_mut().item = 2 };
        assert_eq!(
            unsafe { ins_compl_key2dir(crate::keycodes_defs::K_EVENT) },
            crate::vim_defs::Direction::Backward
        );

        unsafe { crate::popupmenu::PUM_WANT.get_mut().item = 9 };
        assert_eq!(
            unsafe { ins_compl_key2dir(crate::keycodes_defs::K_EVENT) },
            crate::vim_defs::Direction::Forward
        );

        unsafe {
            *crate::popupmenu::PUM_WANT.get_mut() = prev_want;
            *COMPL_SELECTED_ITEM.get_mut() = prev_sel;
        }
    }

    #[test]
    fn ins_compl_pum_key_needs_the_menu_to_be_visible() {
        let _lock = global_state_test_lock();
        let _guard = crate::popupmenu::tests::PumVisibleGuard;

        crate::popupmenu::tests::set_pum_is_visible(false);
        assert!(!ins_compl_pum_key(crate::keycodes_defs::K_PAGEUP));

        crate::popupmenu::tests::set_pum_is_visible(true);
        assert!(ins_compl_pum_key(crate::keycodes_defs::K_PAGEUP));
        // An unrelated key is still not a pum key.
        assert!(!ins_compl_pum_key(i32::from(b'x')));
        assert!(!ins_compl_pum_key(i32::from(crate::ascii_defs::CTRL_P)));
    }

    #[test]
    fn ins_compl_key2count_is_one_for_ordinary_keys() {
        let _lock = global_state_test_lock();
        let _guard = crate::popupmenu::tests::PumVisibleGuard;
        crate::popupmenu::tests::set_pum_is_visible(false);

        assert_eq!(unsafe { ins_compl_key2count(i32::from(b'x')) }, 1);
        assert_eq!(
            unsafe { ins_compl_key2count(i32::from(crate::ascii_defs::CTRL_P)) },
            1
        );
        // Arrow keys move one at a time even with the menu shown.
        crate::popupmenu::tests::set_pum_is_visible(true);
        assert_eq!(unsafe { ins_compl_key2count(crate::keycodes_defs::K_UP) }, 1);
        assert_eq!(unsafe { ins_compl_key2count(crate::keycodes_defs::K_DOWN) }, 1);
    }

    #[test]
    fn ins_compl_key2count_uses_the_menu_height_for_page_keys() {
        let _lock = global_state_test_lock();
        let _guard = crate::popupmenu::tests::PumVisibleGuard;
        crate::popupmenu::tests::set_pum_is_visible(true);
        // PUM_HEIGHT stays 0 in this crate today, so a page key moves
        // by 0 - the height is not clamped up to 1.
        assert_eq!(
            unsafe { ins_compl_key2count(crate::keycodes_defs::K_PAGEUP) },
            crate::popupmenu::pum_get_height()
        );
    }

    #[test]
    fn ins_compl_key2count_is_the_distance_for_external_keys() {
        let _lock = global_state_test_lock();
        let prev_want = unsafe { *crate::popupmenu::PUM_WANT.get_mut() };
        let prev_sel = unsafe { *COMPL_SELECTED_ITEM.get_mut() };
        unsafe { *COMPL_SELECTED_ITEM.get_mut() = 5 };

        // The count is the absolute distance, so it is positive in
        // both directions - the sign is carried by key2dir instead.
        unsafe { crate::popupmenu::PUM_WANT.get_mut().item = 9 };
        assert_eq!(unsafe { ins_compl_key2count(crate::keycodes_defs::K_EVENT) }, 4);

        unsafe { crate::popupmenu::PUM_WANT.get_mut().item = 1 };
        assert_eq!(unsafe { ins_compl_key2count(crate::keycodes_defs::K_EVENT) }, 4);

        unsafe {
            *crate::popupmenu::PUM_WANT.get_mut() = prev_want;
            *COMPL_SELECTED_ITEM.get_mut() = prev_sel;
        }
    }

    #[test]
    fn find_word_start_skips_blanks_and_punctuation() {
        let _lock = global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);

        assert_eq!(unsafe { find_word_start(b"  abc") }, 2);
        // Punctuation is class 1, which is also "not word-ish yet".
        assert_eq!(unsafe { find_word_start(b"...abc") }, 3);
        // Already at a word: no movement.
        assert_eq!(unsafe { find_word_start(b"abc") }, 0);
    }

    #[test]
    fn find_word_start_stops_at_a_newline_or_end() {
        let _lock = global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);

        // The scan stops at '\n' even though there is a word after it.
        assert_eq!(unsafe { find_word_start(b"\nabc") }, 0);
        assert_eq!(unsafe { find_word_start(b"  \nabc") }, 2);
        assert_eq!(unsafe { find_word_start(b"") }, 0);
        // Nothing word-ish at all: runs to the end.
        assert_eq!(unsafe { find_word_start(b"   ") }, 3);
    }

    #[test]
    fn find_word_end_runs_to_the_end_of_the_word() {
        let _lock = global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);

        assert_eq!(unsafe { find_word_end(b"abc def") }, 3);
        // A word running to the end of the slice still terminates.
        assert_eq!(unsafe { find_word_end(b"abc") }, 3);
    }

    #[test]
    fn find_word_end_does_nothing_off_a_word() {
        let _lock = global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);

        // The original guards the whole scan behind `start_class > 1`,
        // so starting off a word returns the start rather than running
        // forward to find one.
        assert_eq!(unsafe { find_word_end(b" abc") }, 0);
        assert_eq!(unsafe { find_word_end(b"...") }, 0);
        assert_eq!(unsafe { find_word_end(b"") }, 0);
    }

    #[test]
    fn find_word_end_stops_when_the_character_class_changes() {
        let _lock = global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);

        // Punctuation (class 1) ends a word run just as a blank does.
        assert_eq!(unsafe { find_word_end(b"abc.def") }, 3);
    }

    #[test]
    fn find_line_end_omits_trailing_cr_and_nl() {
        assert_eq!(find_line_end(b"line\r\n"), 4);
        assert_eq!(find_line_end(b"line\n"), 4);
        assert_eq!(find_line_end(b"line"), 4);
        // Every trailing CR/NL is dropped, not just one.
        assert_eq!(find_line_end(b"line\n\r\n"), 4);
        assert_eq!(find_line_end(b"\r\n"), 0);
        assert_eq!(find_line_end(b""), 0);
    }

    #[test]
    fn find_line_end_stops_at_an_embedded_nul() {
        // The original scans to the NUL terminator, so a NUL inside
        // the slice ends the line rather than being scanned past.
        assert_eq!(find_line_end(b"line\0more"), 4);
        // Trailing CR/NL before the NUL are still trimmed.
        assert_eq!(find_line_end(b"line\r\n\0more"), 4);
    }

    #[test]
    fn ins_compl_accessors_default_to_the_initial_state() {
        let _lock = global_state_test_lock();
        assert!(!unsafe { ins_compl_used_match() });
        assert!(!unsafe { ins_compl_interrupted() });
        assert!(!unsafe { ins_compl_enter_selects() });
        assert_eq!(unsafe { ins_compl_col() }, 0);
        assert_eq!(unsafe { ins_compl_len() }, 0);
    }

    #[test]
    fn ins_compl_col_and_len_report_their_statics() {
        let _lock = global_state_test_lock();
        let (pc, pl) = unsafe { (*COMPL_COL.get_mut(), *COMPL_LENGTH.get_mut()) };

        unsafe {
            *COMPL_COL.get_mut() = 12;
            *COMPL_LENGTH.get_mut() = 5;
        }
        assert_eq!(unsafe { ins_compl_col() }, 12);
        assert_eq!(unsafe { ins_compl_len() }, 5);

        unsafe {
            *COMPL_COL.get_mut() = pc;
            *COMPL_LENGTH.get_mut() = pl;
        }
    }

    #[test]
    fn ins_compl_interrupted_also_covers_an_expired_time_slice() {
        let _lock = global_state_test_lock();
        let (pi, pt) =
            unsafe { (*COMPL_INTERRUPTED.get_mut(), *COMPL_TIME_SLICE_EXPIRED.get_mut()) };

        // Either condition alone counts as interrupted.
        unsafe { *COMPL_INTERRUPTED.get_mut() = true };
        assert!(unsafe { ins_compl_interrupted() });

        unsafe {
            *COMPL_INTERRUPTED.get_mut() = false;
            *COMPL_TIME_SLICE_EXPIRED.get_mut() = true;
        }
        assert!(unsafe { ins_compl_interrupted() });

        unsafe {
            *COMPL_INTERRUPTED.get_mut() = pi;
            *COMPL_TIME_SLICE_EXPIRED.get_mut() = pt;
        }
    }

    #[test]
    fn ins_compl_init_get_longest_clears_the_flag() {
        let _lock = global_state_test_lock();
        let prev = unsafe { *COMPL_GET_LONGEST.get_mut() };

        unsafe { *COMPL_GET_LONGEST.get_mut() = true };
        unsafe { ins_compl_init_get_longest() };
        assert!(!unsafe { *COMPL_GET_LONGEST.get_mut() });

        unsafe { *COMPL_GET_LONGEST.get_mut() = prev };
    }

    #[test]
    fn ins_compl_used_match_and_enter_selects_read_their_own_statics() {
        let _lock = global_state_test_lock();
        let (pu, pe) = unsafe { (*COMPL_USED_MATCH.get_mut(), *COMPL_ENTER_SELECTS.get_mut()) };

        unsafe { *COMPL_USED_MATCH.get_mut() = true };
        assert!(unsafe { ins_compl_used_match() });
        // The two are independent.
        assert!(!unsafe { ins_compl_enter_selects() });

        unsafe { *COMPL_ENTER_SELECTS.get_mut() = true };
        assert!(unsafe { ins_compl_enter_selects() });

        unsafe {
            *COMPL_USED_MATCH.get_mut() = pu;
            *COMPL_ENTER_SELECTS.get_mut() = pe;
        }
    }

    #[test]
    fn get_cot_flags_prefers_a_nonzero_buffer_local_value() {
        let _lock = global_state_test_lock();
        let _cot = CotFlagsGuard::set(crate::option_vars::opt_cot_flag::MENU);
        let mut buf = crate::buffer_defs::BufT {
            b_cot_flags: crate::option_vars::opt_cot_flag::FUZZY,
            ..Default::default()
        };
        let _curbuf = CurbufGuard::set(&mut buf);

        assert_eq!(unsafe { get_cot_flags() }, crate::option_vars::opt_cot_flag::FUZZY);
    }

    #[test]
    fn get_cot_flags_falls_back_to_the_global_when_local_is_zero() {
        let _lock = global_state_test_lock();
        let _cot = CotFlagsGuard::set(crate::option_vars::opt_cot_flag::MENU);
        // Zero means "unset" here, not "no flags".
        let mut buf = crate::buffer_defs::BufT { b_cot_flags: 0, ..Default::default() };
        let _curbuf = CurbufGuard::set(&mut buf);

        assert_eq!(unsafe { get_cot_flags() }, crate::option_vars::opt_cot_flag::MENU);
    }

    #[test]
    fn cot_fuzzy_follows_the_fuzzy_flag() {
        let _guard = CtrlXModeGuard::set(CTRL_X_NORMAL);
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);

        let _cot = CotFlagsGuard::set(crate::option_vars::opt_cot_flag::FUZZY);
        assert!(unsafe { cot_fuzzy() });
        drop(_cot);

        let _cot = CotFlagsGuard::set(crate::option_vars::opt_cot_flag::MENU);
        assert!(!unsafe { cot_fuzzy() });
    }

    #[test]
    fn cot_fuzzy_is_off_in_thesaurus_mode() {
        // Thesaurus matches are looked up rather than filtered, so
        // fuzzy matching opts out even with the flag set.
        let _guard = CtrlXModeGuard::set(CTRL_X_THESAURUS);
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);
        let _cot = CotFlagsGuard::set(crate::option_vars::opt_cot_flag::FUZZY);

        assert!(unsafe { ctrl_x_mode_thesaurus() });
        assert!(!unsafe { cot_fuzzy() });
    }

    #[test]
    fn is_nearest_active_follows_the_nearest_flag() {
        let _guard = CtrlXModeGuard::set(CTRL_X_NORMAL);
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);

        let _cot = CotFlagsGuard::set(crate::option_vars::opt_cot_flag::NEAREST);
        assert!(unsafe { is_nearest_active() });
        drop(_cot);

        let _cot = CotFlagsGuard::set(crate::option_vars::opt_cot_flag::MENU);
        assert!(!unsafe { is_nearest_active() });
    }

    #[test]
    fn is_nearest_active_yields_to_fuzzy_matching() {
        // Fuzzy imposes its own ordering, so it wins outright.
        let _guard = CtrlXModeGuard::set(CTRL_X_NORMAL);
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);
        let _cot = CotFlagsGuard::set(
            crate::option_vars::opt_cot_flag::NEAREST | crate::option_vars::opt_cot_flag::FUZZY,
        );

        assert!(!unsafe { is_nearest_active() });
    }

    #[test]
    fn ins_compl_preinsert_longest_needs_autocomplete() {
        let _guard = CtrlXModeGuard::set(CTRL_X_NORMAL);
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);
        let _cot = CotFlagsGuard::set(crate::option_vars::opt_cot_flag::LONGEST);

        // Autocomplete is off by default, so the flag alone is not
        // enough.
        assert!(!unsafe { ins_compl_preinsert_longest() });

        unsafe { *COMPL_AUTOCOMPLETE.get_mut() = true };
        assert!(unsafe { ins_compl_preinsert_longest() });
        unsafe { *COMPL_AUTOCOMPLETE.get_mut() = false };
    }

    #[test]
    fn ins_compl_preinsert_longest_wants_longest_alone() {
        let _guard = CtrlXModeGuard::set(CTRL_X_NORMAL);
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);
        unsafe { *COMPL_AUTOCOMPLETE.get_mut() = true };

        use crate::option_vars::opt_cot_flag::{FUZZY, LONGEST, MENU, PREINSERT};
        // The original masks longest|preinsert|fuzzy and compares the
        // whole result to longest, so either companion flag disables
        // it - it is not merely "is longest set".
        for (flags, expected) in [
            (LONGEST, true),
            // An unrelated flag does not disturb the masked compare.
            (LONGEST | MENU, true),
            (LONGEST | PREINSERT, false),
            (LONGEST | FUZZY, false),
            (PREINSERT, false),
            (0, false),
        ] {
            let _cot = CotFlagsGuard::set(flags);
            assert_eq!(
                unsafe { ins_compl_preinsert_longest() },
                expected,
                "flags {flags:#x}"
            );
        }

        unsafe { *COMPL_AUTOCOMPLETE.get_mut() = false };
    }

    #[test]
    fn compl_cont_status_defaults_to_no_flags_set() {
        let _guard = ComplStateGuard::new();
        assert!(!unsafe { compl_status_adding() });
        assert!(!unsafe { compl_status_sol() });
        assert!(!unsafe { compl_status_local() });
    }

    #[test]
    fn compl_status_predicates_each_read_their_own_flag() {
        let _guard = ComplStateGuard::new();

        unsafe { *COMPL_CONT_STATUS.get_mut() = CONT_ADDING };
        assert!(unsafe { compl_status_adding() });
        assert!(!unsafe { compl_status_sol() });
        assert!(!unsafe { compl_status_local() });

        unsafe { *COMPL_CONT_STATUS.get_mut() = CONT_SOL };
        assert!(!unsafe { compl_status_adding() });
        assert!(unsafe { compl_status_sol() });

        unsafe { *COMPL_CONT_STATUS.get_mut() = CONT_LOCAL };
        assert!(!unsafe { compl_status_sol() });
        assert!(unsafe { compl_status_local() });
    }

    #[test]
    fn compl_status_flags_combine() {
        let _guard = ComplStateGuard::new();
        unsafe { *COMPL_CONT_STATUS.get_mut() = CONT_ADDING | CONT_SOL | CONT_LOCAL };
        assert!(unsafe { compl_status_adding() });
        assert!(unsafe { compl_status_sol() });
        assert!(unsafe { compl_status_local() });
    }

    #[test]
    fn cont_intrpt_implies_cont_n_adds() {
        // CONT_INTRPT is deliberately 2 + 4 in the original, so it
        // carries CONT_N_ADDS with it rather than being a lone bit.
        assert_eq!(CONT_INTRPT, 6);
        assert_ne!(CONT_INTRPT & CONT_N_ADDS, 0);
    }

    #[test]
    fn cont_flags_are_distinct_bits() {
        // Every flag but CONT_INTRPT is a single, distinct bit.
        for (a, b) in [
            (CONT_ADDING, CONT_N_ADDS),
            (CONT_ADDING, CONT_S_IPOS),
            (CONT_N_ADDS, CONT_S_IPOS),
            (CONT_S_IPOS, CONT_SOL),
            (CONT_SOL, CONT_LOCAL),
        ] {
            assert_eq!(a & b, 0, "{a} and {b} overlap");
        }
    }

    #[test]
    fn compl_status_clear_resets_every_flag() {
        let _guard = ComplStateGuard::new();
        unsafe { *COMPL_CONT_STATUS.get_mut() = CONT_ADDING | CONT_SOL | CONT_LOCAL };
        unsafe { compl_status_clear() };
        assert_eq!(unsafe { *COMPL_CONT_STATUS.get_mut() }, 0);
        assert!(!unsafe { compl_status_adding() });
    }

    #[test]
    fn completion_directions_default_to_forward() {
        let _guard = ComplStateGuard::new();
        assert!(unsafe { compl_dir_forward() });
        assert!(unsafe { compl_shows_dir_forward() });
        assert!(!unsafe { compl_shows_dir_backward() });
    }

    #[test]
    fn compl_dir_forward_is_independent_of_the_shown_direction() {
        let _guard = ComplStateGuard::new();
        unsafe { *COMPL_DIRECTION.get_mut() = crate::vim_defs::Direction::Backward };
        assert!(!unsafe { compl_dir_forward() });
        // The direction being shown is tracked separately.
        assert!(unsafe { compl_shows_dir_forward() });
    }

    #[test]
    fn shows_dir_forward_and_backward_can_both_be_false() {
        let _guard = ComplStateGuard::new();
        // Direction also has the *_FILE values, so these two
        // predicates are not each other's negation.
        unsafe { *COMPL_SHOWS_DIR.get_mut() = crate::vim_defs::Direction::ForwardFile };
        assert!(!unsafe { compl_shows_dir_forward() });
        assert!(!unsafe { compl_shows_dir_backward() });

        unsafe { *COMPL_SHOWS_DIR.get_mut() = crate::vim_defs::Direction::Backward };
        assert!(unsafe { compl_shows_dir_backward() });
    }

    #[test]
    fn defaults_to_ctrl_x_normal() {
        let _lock = global_state_test_lock();
        assert_eq!(unsafe { ctrl_x_mode() }, CTRL_X_NORMAL);
        assert!(unsafe { ctrl_x_mode_normal() });
        assert!(unsafe { ctrl_x_mode_none() }); // CTRL_X_NORMAL == 0
        assert!(!unsafe { ctrl_x_mode_not_default() });
    }

    #[test]
    fn ins_compl_active_defaults_to_false() {
        let _lock = global_state_test_lock();
        assert!(!unsafe { ins_compl_active() });
    }

    #[test]
    fn ins_compl_active_reflects_compl_started() {
        // Directly manipulate the file-static (something no real,
        // translated caller can currently do, since nothing starts a
        // real completion session yet) to prove ins_compl_active
        // reads the REAL value, not a hardcoded false.
        let _lock = global_state_test_lock();
        unsafe { *COMPL_STARTED.get_mut() = true };
        assert!(unsafe { ins_compl_active() });
        unsafe { *COMPL_STARTED.get_mut() = false };
        assert!(!unsafe { ins_compl_active() });
    }

    #[test]
    fn get_mode_insert_reports_c_when_completion_is_active() {
        // Proves state.rs's get_mode() now calls the REAL
        // ins_compl_active() (wired in this same update), not a
        // hardcoded false - manipulates COMPL_STARTED directly, only
        // possible from within this same module (it's a private
        // static), so this test lives here rather than in state.rs.
        let _guard = CtrlXModeGuard::set(CTRL_X_NORMAL);
        let mut buf = crate::buffer_defs::BufT::default();
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut crate::buffer_defs::BufT;
        let prev_state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
        unsafe { crate::globals::GLOBALS.get_mut() }.State = crate::state_defs::mode::INSERT as i32;
        unsafe { *COMPL_STARTED.get_mut() = true };

        let result = unsafe { crate::state::get_mode() };

        unsafe { *COMPL_STARTED.get_mut() = false };
        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;

        assert_eq!(result, b"ic".to_vec());
    }

    #[test]
    fn get_mode_insert_reports_x_when_ctrl_x_mode_not_defined_yet() {
        let _guard = CtrlXModeGuard::set(CTRL_X_NOT_DEFINED_YET);
        let mut buf = crate::buffer_defs::BufT::default();
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut crate::buffer_defs::BufT;
        let prev_state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
        unsafe { crate::globals::GLOBALS.get_mut() }.State = crate::state_defs::mode::INSERT as i32;

        let result = unsafe { crate::state::get_mode() };

        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;

        assert_eq!(result, b"ix".to_vec());
    }

    #[test]
    fn ctrl_x_mode_scroll_matches_only_scroll() {
        let _guard = CtrlXModeGuard::set(CTRL_X_SCROLL);
        assert!(unsafe { ctrl_x_mode_scroll() });
        assert!(!unsafe { ctrl_x_mode_normal() });
        assert!(unsafe { ctrl_x_mode_not_default() });
    }

    #[test]
    fn ctrl_x_mode_whole_line_and_line_or_eval() {
        let _guard = CtrlXModeGuard::set(CTRL_X_WHOLE_LINE);
        assert!(unsafe { ctrl_x_mode_whole_line() });
        assert!(unsafe { ctrl_x_mode_line_or_eval() });
        assert!(!unsafe { ctrl_x_mode_eval() });
    }

    #[test]
    fn ctrl_x_mode_eval_and_line_or_eval() {
        let _guard = CtrlXModeGuard::set(CTRL_X_EVAL);
        assert!(unsafe { ctrl_x_mode_eval() });
        assert!(unsafe { ctrl_x_mode_line_or_eval() });
        assert!(!unsafe { ctrl_x_mode_whole_line() });
    }

    #[test]
    fn ctrl_x_mode_files_tags_path_patterns_path_defines() {
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_FILES);
            assert!(unsafe { ctrl_x_mode_files() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_TAGS);
            assert!(unsafe { ctrl_x_mode_tags() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_PATH_PATTERNS);
            assert!(unsafe { ctrl_x_mode_path_patterns() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_PATH_DEFINES);
            assert!(unsafe { ctrl_x_mode_path_defines() });
        }
    }

    #[test]
    fn ctrl_x_mode_dictionary_and_thesaurus() {
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_DICTIONARY);
            assert!(unsafe { ctrl_x_mode_dictionary() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_THESAURUS);
            assert!(unsafe { ctrl_x_mode_thesaurus() });
        }
    }

    #[test]
    fn ctrl_x_mode_cmdline_matches_both_cmdline_variants() {
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_CMDLINE);
            assert!(unsafe { ctrl_x_mode_cmdline() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_CMDLINE_CTRL_X);
            assert!(unsafe { ctrl_x_mode_cmdline() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_NORMAL);
            assert!(!unsafe { ctrl_x_mode_cmdline() });
        }
    }

    #[test]
    fn ctrl_x_mode_function_omni_spell_register() {
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_FUNCTION);
            assert!(unsafe { ctrl_x_mode_function() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_OMNI);
            assert!(unsafe { ctrl_x_mode_omni() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_SPELL);
            assert!(unsafe { ctrl_x_mode_spell() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_REGISTER);
            assert!(unsafe { ctrl_x_mode_register() });
        }
    }

    #[test]
    fn ctrl_x_mode_not_defined_yet_matches_only_that_state() {
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_NOT_DEFINED_YET);
            assert!(unsafe { ctrl_x_mode_not_defined_yet() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_NORMAL);
            assert!(!unsafe { ctrl_x_mode_not_defined_yet() });
        }
    }

    #[test]
    fn ctrl_x_constants_match_the_real_source_values() {
        // Mechanically re-derived from the original's own enum +
        // CTRL_X_WANT_IDENT = 0x100 definition, cross-checked directly
        // against the real source before trusting these.
        assert_eq!(CTRL_X_NORMAL, 0);
        assert_eq!(CTRL_X_NOT_DEFINED_YET, 1);
        assert_eq!(CTRL_X_SCROLL, 2);
        assert_eq!(CTRL_X_WHOLE_LINE, 3);
        assert_eq!(CTRL_X_FILES, 4);
        assert_eq!(CTRL_X_TAGS, 5 + 0x100);
        assert_eq!(CTRL_X_PATH_PATTERNS, 6 + 0x100);
        assert_eq!(CTRL_X_PATH_DEFINES, 7 + 0x100);
        assert_eq!(CTRL_X_FINISHED, 8);
        assert_eq!(CTRL_X_DICTIONARY, 9 + 0x100);
        assert_eq!(CTRL_X_THESAURUS, 10 + 0x100);
        assert_eq!(CTRL_X_CMDLINE, 11);
        assert_eq!(CTRL_X_FUNCTION, 12);
        assert_eq!(CTRL_X_OMNI, 13);
        assert_eq!(CTRL_X_SPELL, 14);
        assert_eq!(CTRL_X_LOCAL_MSG, 15);
        assert_eq!(CTRL_X_EVAL, 16);
        assert_eq!(CTRL_X_CMDLINE_CTRL_X, 17);
        assert_eq!(CTRL_X_BUFNAMES, 18);
        assert_eq!(CTRL_X_REGISTER, 19);
    }

    #[test]
    fn set_ref_in_insexpand_funcs_is_always_false_since_every_callback_stays_empty() {
        // Nothing in this crate can populate CFU_CB/OFU_CB/TSRFU_CB/
        // CPT_CB with a real callback yet (needs
        // option_set_callback_func) - they always stay at their own
        // empty defaults, matching a real, unconfigured session.
        let _lock = global_state_test_lock();
        assert!(!unsafe { set_ref_in_insexpand_funcs(1) });
    }

    #[test]
    fn set_ref_in_cpt_callbacks_empty_slice_is_always_false() {
        assert!(!unsafe { set_ref_in_cpt_callbacks(&[], 1) });
    }

    #[test]
    fn set_ref_in_cpt_callbacks_none_callbacks_are_always_false() {
        let callbacks = [
            crate::eval::typval_defs::Callback::None,
            crate::eval::typval_defs::Callback::Funcref(b"MyFunc".to_vec()),
        ];
        assert!(!unsafe { set_ref_in_cpt_callbacks(&callbacks, 1) });
    }
}
