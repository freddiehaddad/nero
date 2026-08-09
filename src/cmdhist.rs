//! Translated from `src/nvim/cmdhist.c` (tractable core only).
//!
//! `cmdhist.c` manages the command-line history (`:history`, up/down
//! arrow recall) - almost entirely dependent on the command-line
//! editing subsystem (`ex_getln.c`, which explicitly does not attempt
//! the history-editing machinery either - see that module's own doc
//! comment).
//!
//! Translated: [`HistoryType`], [`HIST_COUNT`], [`get_hislen`] (reading
//! a new file-static `HISLEN`, still `0` until `init_history` lands),
//! [`hist_char2type`] - a pure character-to-enum mapping with no
//! dependencies at all - and [`get_histtype`]/[`get_history_idx`]/
//! [`f_histnr`] (`histnr()`), tractable once `HISLEN` existed and
//! `ex_getln.rs` gained `get_cmdline_firstc()`.
//!
//! Also the history tables themselves - `HistentryT`, `HISTORY`,
//! `HISIDX`, `HISNUM` - plus [`clear_hist_entry`],
//! [`hist_free_entry`] and [`in_history`]. `hisstr` is an
//! `Option<Vec<u8>>` so that the original's own `NULL` (an unused ring
//! slot, which several loops stop on) stays distinguishable from an
//! empty string. `hisstrlen` is kept as its own field rather than
//! derived, because the original stores a search history entry's
//! separator character one byte PAST that length - which is exactly
//! how `in_history` checks it.
//!
//! `hist_free_entry`'s two `xfree` calls have no counterpart: dropping
//! the owned `Option`s is what frees them, so it reduces to
//! `clear_hist_entry` plus the ownership release that implies.
//!
//! Also `calc_hist_idx`, [`f_histget`] (`histget()`) and
//! `del_history_idx`, all completed once the history tables landed.
//! `calc_hist_idx` and `f_histget` previously had `unimplemented!()`
//! bodies documented as unreachable "while `hislen` stays 0"; the
//! tables exist now, so those notes were stale and both functions are
//! real. `f_histget` copies only `hisstrlen` bytes, NOT the whole
//! stored buffer - a search entry keeps its separator character after
//! the entry's own NUL, and that must not leak into the returned
//! string.
//!
//! `del_history_idx` brings `last_maptick` with it (a file-static in
//! the original, kept file-local here). `del_history_entry` stays
//! deferred: it needs `vim_regcomp`/`vim_regexec`, and the regex
//! engine is not translated.
//!
//! Also the table accessors `get_histentry`/`set_histentry`/
//! `get_hisidx`/`get_hisnum` and `clr_history`. The original's
//! accessors return raw `histentry_T *`/`int *` into the file-static
//! arrays so callers can both read and write; each splits into a
//! getter plus a `set_*` counterpart here, which is the same
//! capability without handing out a pointer into a mutable static.
//!
//! Also `init_history`, which resizes the tables to match
//! `'history'`. On resize the original REORDERS the ring into a plain
//! oldest-first table (copying the oldest run first, then the newest)
//! and leaves `hisidx` on the last copied entry; that reordering is
//! preserved and pinned by its own grow/shrink tests. The original's
//! `memset(temp + l3, 0, ...)` needs no counterpart - the new table is
//! built from `HistentryT::default()`, which is already that.
//!
//! Also `add_to_history`, the last piece needed for entries to
//! actually be stored. The separator is written one byte PAST the
//! entry's own NUL (the original allocates `new_entrylen + 2` for
//! exactly this), which is what `in_history` reads back at
//! `hisstrlen + 1`. Two behaviours are preserved and pinned by tests:
//! `:keeppatterns` suppresses only SEARCH history, and consecutive
//! searches from within one mapping overwrite each other via the
//! `maptick`/`last_maptick` comparison rather than each consuming a
//! ring slot.
//!
//! Also `get_history_arg`, which yields `:history`'s own argument
//! completion candidates. The original writes a one-character short
//! name into the caller's `xp->xp_buf` and returns that pointer purely
//! for somewhere to put it; an owned `Vec<u8>` needs no scratch space,
//! so `xp` is not a parameter here at all.
//!
//! Also translated: [`hist_iter`], the iterator over one history
//! ring that `shada.c`'s history merger drives. The original hands
//! the caller an opaque `const void *` into the ring; an index is
//! used here instead, which needs no sentinel arithmetic and cannot
//! outlive a reallocation of the table.
//!
//! Deferred: `f_histdel`/`del_history_entry` (need the untranslated
//! regex engine) and `ex_history` (needs message display).

use crate::globals::GlobalCell;

/// Present history tables (`HistoryType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum HistoryType {
    /// Default (current) history.
    Default = -2,
    /// Unknown history.
    Invalid = -1,
    /// Colon commands.
    Cmd = 0,
    /// Search commands.
    Search = 1,
    /// Expressions (e.g. from entering a `=` register).
    Expr = 2,
    /// `input()` lines.
    Input = 3,
    /// Debug commands.
    Debug = 4,
}

/// Number of history tables (`HIST_COUNT`).
pub const HIST_COUNT: usize = 5;

/// actual length of the history tables (`hislen`).
static HISLEN: GlobalCell<i32> = GlobalCell::new(0);

/// One command-line history entry (`histentry_T`, `cmdhist.h`).
#[derive(Debug, Clone, Default)]
pub struct HistentryT {
    /// Entry identifier number.
    pub hisnum: i32,
    /// Actual entry. `None` models the original's own `NULL`, which
    /// marks an unused slot in the ring - it is NOT the same as an
    /// empty string, and several loops here stop on it.
    pub hisstr: Option<Vec<u8>>,
    /// Length of `hisstr` (excluding the NUL). Kept as its own field
    /// rather than derived, because the original stores the search
    /// separator character one byte PAST that length.
    pub hisstrlen: usize,
    /// Time when entry was added.
    pub timestamp: crate::os::time_defs::Timestamp,
    /// Additional entries from a ShaDa file.
    pub additional_data: Option<crate::types_defs::AdditionalData>,
}

/// The history tables themselves (`history`), one ring buffer per
/// [`HistoryType`].
static HISTORY: GlobalCell<[Vec<HistentryT>; HIST_COUNT]> =
    GlobalCell::new([Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()]);

/// Index of the last used entry in each table (`hisidx`), `-1` when
/// the table is empty.
static HISIDX: GlobalCell<[i32; HIST_COUNT]> = GlobalCell::new([-1; HIST_COUNT]);

/// Identifying (unique) number of the newest history entry in each
/// table (`hisnum`).
static HISNUM: GlobalCell<[i32; HIST_COUNT]> = GlobalCell::new([0; HIST_COUNT]);

/// Clear one history entry (`clear_hist_entry`).
///
/// The original's `CLEAR_POINTER` zeroes the whole struct, which is
/// exactly `Default`.
pub fn clear_hist_entry(hisptr: &mut HistentryT) {
    *hisptr = HistentryT::default();
}

/// Free one history entry and clear it (`hist_free_entry`).
///
/// The original's two `xfree` calls have no counterpart - dropping
/// the owned `Option`s is what frees them - so this is
/// [`clear_hist_entry`] plus the ownership release it implies.
pub fn hist_free_entry(hisptr: &mut HistentryT) {
    clear_hist_entry(hisptr);
}

/// Whether command line `str` is already in history
/// (`in_history`).
///
/// When `move_to_front` is set, a matching entry is rotated to the
/// end of the history ring and given a fresh entry number and
/// timestamp. For [`HistoryType::Search`] the separator character
/// must match too - the original stores it one byte past the entry's
/// own NUL, so it is read at `hisstrlen + 1`.
///
/// # Safety
/// Must not run concurrently with any other access to the history
/// tables.
pub unsafe fn in_history(
    htype: HistoryType,
    str: &[u8],
    move_to_front: bool,
    sep: i32,
) -> bool {
    let Ok(t) = usize::try_from(htype as i32) else {
        return false;
    };
    if t >= HIST_COUNT {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let hisidx = unsafe { HISIDX.get_mut() };
    if hisidx[t] < 0 {
        return false;
    }
    let hislen = get_hislen();
    if hislen <= 0 {
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let history = unsafe { HISTORY.get_mut() };
    let mut last_i: i32 = -1;
    let mut i = hisidx[t];
    loop {
        let Some(entry) = history[t].get(i as usize) else {
            return false;
        };
        let Some(p) = entry.hisstr.as_ref() else {
            return false;
        };

        // For search history, check that the separator character
        // matches as well.
        let sep_ok = htype != HistoryType::Search
            || p.get(entry.hisstrlen + 1).map(|&c| i32::from(c)) == Some(sep);
        if p.as_slice() == str && sep_ok {
            if !move_to_front {
                return true;
            }
            last_i = i;
            break;
        }
        i -= 1;
        if i < 0 {
            i = hislen - 1;
        }
        if i == hisidx[t] {
            break;
        }
    }

    if last_i < 0 {
        return false;
    }

    let saved = history[t][i as usize].clone();
    while i != hisidx[t] {
        i += 1;
        if i >= hislen {
            i = 0;
        }
        history[t][last_i as usize] = history[t][i as usize].clone();
        last_i = i;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let hisnum = unsafe { HISNUM.get_mut() };
    hisnum[t] += 1;
    let slot = &mut history[t][i as usize];
    slot.hisnum = hisnum[t];
    slot.hisstr = saved.hisstr;
    slot.hisstrlen = saved.hisstrlen;
    slot.timestamp = crate::os::time::os_time();
    // The original frees the old `additional_data` here and stores
    // NULL; dropping the value does the same.
    slot.additional_data = None;
    true
}

/// The history table for `hist_type` (`get_histentry`).
///
/// The original returns a raw `histentry_T *` into the file-static
/// array so callers can both read and write it; here that splits into
/// this read-only view plus [`set_histentry`], which is the same
/// capability without handing out a pointer into a mutable static.
///
/// # Safety
/// Must not run concurrently with any other access to the history
/// tables.
#[must_use]
pub unsafe fn get_histentry(hist_type: HistoryType) -> Vec<HistentryT> {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { HISTORY.get_mut() })[hist_type as usize].clone()
}

/// Replace the history table for `hist_type` (`set_histentry`).
///
/// # Safety
/// Forwarded from [`get_histentry`]'s own safety doc.
pub unsafe fn set_histentry(hist_type: HistoryType, entry: Vec<HistentryT>) {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { HISTORY.get_mut() })[hist_type as usize] = entry;
}

/// The last-used index for `hist_type` (`get_hisidx`).
///
/// # Safety
/// Forwarded from [`get_histentry`]'s own safety doc.
#[must_use]
pub unsafe fn get_hisidx(hist_type: HistoryType) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { HISIDX.get_mut() })[hist_type as usize]
}

/// Set the last-used index for `hist_type` - the write half of the
/// original's own `int *get_hisidx(int)`.
///
/// # Safety
/// Forwarded from [`get_histentry`]'s own safety doc.
pub unsafe fn set_hisidx(hist_type: HistoryType, value: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { HISIDX.get_mut() })[hist_type as usize] = value;
}

/// The newest entry number for `hist_type` (`get_hisnum`).
///
/// # Safety
/// Forwarded from [`get_histentry`]'s own safety doc.
#[must_use]
pub unsafe fn get_hisnum(hist_type: HistoryType) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { HISNUM.get_mut() })[hist_type as usize]
}

/// Set the newest entry number for `hist_type` - the write half of
/// the original's own `int *get_hisnum(int)`.
///
/// # Safety
/// Forwarded from [`get_histentry`]'s own safety doc.
pub unsafe fn set_hisnum(hist_type: HistoryType, value: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { HISNUM.get_mut() })[hist_type as usize] = value;
}

/// Clear all entries in a history (`clr_history`).
///
/// Returns `OK` when there was something to clear and `histype` was a
/// real history type, `FAIL` otherwise.
///
/// # Safety
/// Forwarded from [`get_histentry`]'s own safety doc.
pub unsafe fn clr_history(histype: HistoryType) -> i32 {
    let hislen = get_hislen();
    let t = histype as i32;
    if hislen != 0 && t >= 0 && t < HIST_COUNT as i32 {
        let t = t as usize;
        // SAFETY: forwarded from this function's own safety doc.
        let history = unsafe { HISTORY.get_mut() };
        // The original walks exactly `hislen` slots; the table is
        // sized to `hislen` once `init_history` has run, so iterating
        // it is the same walk without risking an out-of-bounds one.
        for e in &mut history[t] {
            hist_free_entry(e);
        }
        // SAFETY: forwarded from this function's own safety doc.
        (unsafe { HISIDX.get_mut() })[t] = -1; // mark history as cleared
        // SAFETY: forwarded from this function's own safety doc.
        (unsafe { HISNUM.get_mut() })[t] = 0; // reset identifier counter
        return crate::vim_defs::OK;
    }
    crate::vim_defs::FAIL
}

/// Resize the history tables to match `'history'` (`init_history`).
///
/// The tables are circular arrays whose current position is marked by
/// `hisidx[type]`. On resize the original reallocates and takes the
/// chance to REORDER them, so the new table is laid out oldest-first
/// with `hisidx` pointing at the last copied entry. Entries that no
/// longer fit are freed.
///
/// # Safety
/// Must not run concurrently with any other access to the history
/// tables or to `OPTION_VARS`.
pub unsafe fn init_history() {
    // SAFETY: forwarded from this function's own safety doc.
    let p_hi = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hi;
    let newlen = i32::try_from(p_hi).unwrap_or(0).max(0);
    let oldlen = get_hislen();

    if newlen == oldlen {
        // history length didn't change
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let history = unsafe { HISTORY.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let hisidx = unsafe { HISIDX.get_mut() };

    for t in 0..HIST_COUNT {
        let mut temp: Vec<HistentryT> = vec![HistentryT::default(); newlen.max(0) as usize];

        let j = hisidx[t];
        // Number of entries actually carried over.
        let mut l3 = 0;
        if j >= 0 {
            // The old array partitions as:
            //   [0        , i1     ) newest entries to be deleted
            //   [i1       , i1 + l1) newest entries to be copied
            //   [i1 + l1  , i2     ) oldest entries to be deleted
            //   [i2       , i2 + l2) oldest entries to be copied
            let l1 = (j + 1).min(newlen); // how many newest to copy
            let l2 = newlen.min(oldlen) - l1; // how many oldest to copy
            let i1 = j + 1 - l1; // copy newest from here
            let i2 = l1.max(oldlen - newlen + l1); // copy oldest from here

            if newlen > 0 {
                // Copy oldest entries, then newest - this is what
                // reorders the ring into a plain oldest-first table.
                for k in 0..l2 {
                    if let Some(e) = history[t].get((i2 + k) as usize) {
                        temp[k as usize] = e.clone();
                    }
                }
                for k in 0..l1 {
                    if let Some(e) = history[t].get((i1 + k) as usize) {
                        temp[(l2 + k) as usize] = e.clone();
                    }
                }
            }

            // Delete entries that don't fit in newlen, if any.
            for i in 0..i1 {
                if let Some(e) = history[t].get_mut(i as usize) {
                    hist_free_entry(e);
                }
            }
            for i in (i1 + l1)..i2 {
                if let Some(e) = history[t].get_mut(i as usize) {
                    hist_free_entry(e);
                }
            }

            l3 = newlen.min(oldlen);
        }

        // The remaining space is already cleared: `temp` was built
        // from `HistentryT::default()`, which is the original's own
        // `memset(temp + l3, 0, ...)`.
        hisidx[t] = l3 - 1;
        history[t] = std::mem::take(&mut temp);
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *HISLEN.get_mut() = newlen };
}

/// Add the given string to a history (`add_to_history`).
///
/// `in_map` marks the entry as coming from inside a mapping;
/// consecutive searches within one mapping overwrite each other so
/// only the last is kept. `sep` is the search separator character,
/// which the original stores one byte PAST the entry's own NUL - so
/// the buffer allocated is `new_entrylen + 2` bytes.
///
/// # Safety
/// Must not run concurrently with any other access to the history
/// tables, `GLOBALS` or `OPTION_VARS`.
pub unsafe fn add_to_history(
    histype: HistoryType,
    new_entry: &[u8],
    in_map: bool,
    sep: i32,
) {
    let hislen = get_hislen();
    if hislen == 0 || histype == HistoryType::Invalid {
        // no history
        return;
    }
    debug_assert!(histype != HistoryType::Default);
    let t = histype as usize;

    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    if (globals.cmdmod.cmod_flags & crate::ex_cmds_defs::cmod::KEEPPATTERNS) != 0
        && histype == HistoryType::Search
    {
        return;
    }
    let maptick = globals.maptick;

    // Searches inside the same mapping overwrite each other, so that
    // only the last line is kept. Be careful not to remove a line that
    // was moved down, only lines that were added.
    if histype == HistoryType::Search && in_map {
        // SAFETY: forwarded from this function's own safety doc.
        let last_maptick = unsafe { LAST_MAPTICK.get_mut() };
        let s = HistoryType::Search as usize;
        // SAFETY: forwarded from this function's own safety doc.
        let hisidx = unsafe { HISIDX.get_mut() };
        if maptick == *last_maptick && hisidx[s] >= 0 {
            // Current line is from the same mapping, remove it.
            // SAFETY: forwarded from this function's own safety doc.
            let history = unsafe { HISTORY.get_mut() };
            if let Some(e) = history[s].get_mut(hisidx[s] as usize) {
                hist_free_entry(e);
            }
            // SAFETY: forwarded from this function's own safety doc.
            (unsafe { HISNUM.get_mut() })[t] -= 1;
            hisidx[s] -= 1;
            if hisidx[s] < 0 {
                hisidx[s] = hislen - 1;
            }
        }
        *last_maptick = -1;
    }

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { in_history(histype, new_entry, true, sep) } {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let hisidx = unsafe { HISIDX.get_mut() };
    hisidx[t] += 1;
    if hisidx[t] == hislen {
        hisidx[t] = 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let history = unsafe { HISTORY.get_mut() };
    let Some(hisptr) = history[t].get_mut(hisidx[t] as usize) else {
        return;
    };
    hist_free_entry(hisptr);

    // Store the separator after the NUL of the string - the original
    // allocates `new_entrylen + 2` bytes for exactly this.
    let mut buf = new_entry.to_vec();
    buf.resize(new_entry.len() + 2, 0);
    buf[new_entry.len() + 1] = u8::try_from(sep).unwrap_or(0);
    hisptr.hisstr = Some(buf);
    hisptr.timestamp = crate::os::time::os_time();
    hisptr.additional_data = None;
    hisptr.hisstrlen = new_entry.len();

    // SAFETY: forwarded from this function's own safety doc.
    let hisnum = unsafe { HISNUM.get_mut() };
    hisnum[t] += 1;
    history[t][hisidx[t] as usize].hisnum = hisnum[t];

    if histype == HistoryType::Search && in_map {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { *LAST_MAPTICK.get_mut() = maptick };
    }
}

/// Returns the length of the history tables (`get_hislen`).
///
/// Still `0` until `init_history`/`add_to_history` land: those are
/// the only things that size the tables, so `HISLEN` never becomes
/// anything else on its own - a real, faithful consequence of the
/// current state, not a hardcoded stub. Tests can set it via the
/// test-only `set_hislen` helper.
#[must_use]
pub fn get_hislen() -> i32 {
    // SAFETY: a plain read through one exclusive borrow.
    unsafe { *HISLEN.get_mut() }
}

/// Translates a history character (`:`, `=`, `@`, `>`, `/`, `?`, or NUL)
/// to its associated [`HistoryType`], or [`HistoryType::Invalid`] for
/// anything else (`hist_char2type`).
#[must_use]
pub fn hist_char2type(c: i32) -> HistoryType {
    if c == i32::from(b':') {
        HistoryType::Cmd
    } else if c == i32::from(b'=') {
        HistoryType::Expr
    } else if c == i32::from(b'@') {
        HistoryType::Input
    } else if c == i32::from(b'>') {
        HistoryType::Debug
    } else if c == 0 || c == i32::from(b'/') || c == i32::from(b'?') {
        HistoryType::Search
    } else {
        HistoryType::Invalid
    }
}

/// One completion candidate for `:history`'s own argument
/// (`get_history_arg`), or `None` once `idx` runs past the end.
///
/// The candidates are, in order: the six short history names
/// (`:=@>?/`), then the long names from `HISTORY_NAMES`, then
/// `"all"`.
///
/// The original writes a single short name into the caller's own
/// `xp->xp_buf` and returns that pointer, purely so it has somewhere
/// to put a one-character string; an owned `Vec<u8>` needs no such
/// scratch space, so `xp` is not a parameter here at all.
#[must_use]
pub fn get_history_arg(idx: i32) -> Option<Vec<u8>> {
    const SHORT_NAMES: &[u8] = b":=@>?/";
    let short_count = SHORT_NAMES.len() as i32;
    let name_count = HISTORY_NAMES.len() as i32;

    if idx < 0 {
        return None;
    }
    if idx < short_count {
        return Some(vec![SHORT_NAMES[idx as usize]]);
    }
    if idx < short_count + name_count {
        return Some(HISTORY_NAMES[(idx - short_count) as usize].to_vec());
    }
    if idx == short_count + name_count {
        return Some(b"all".to_vec());
    }
    None
}

/// Table of history names (`history_names[]`), used by
/// [`get_histtype`] and (in the original) `:history`'s own argument
/// completion.
const HISTORY_NAMES: [&[u8]; HIST_COUNT] = [b"cmd", b"search", b"expr", b"input", b"debug"];

/// The corresponding [`HistoryType`] for each entry of
/// `HISTORY_NAMES`, in the same order.
const HISTORY_NAME_TYPES: [HistoryType; HIST_COUNT] = [
    HistoryType::Cmd,
    HistoryType::Search,
    HistoryType::Expr,
    HistoryType::Input,
    HistoryType::Debug,
];

/// Parses a history-name argument into its [`HistoryType`]
/// (`get_histtype`) - a case-insensitive prefix match against
/// `HISTORY_NAMES` (e.g. `"s"`/`"sea"`/`"search"` all match
/// `Search`), or a single history-marker character (`:`/`=`/`@`/`>`/
/// `/`/`?`). An empty `name` resolves to either [`HistoryType::Default`]
/// (when `return_default` is set) or the CURRENT command line's own
/// leading character via [`crate::ex_getln::get_cmdline_firstc`]/
/// [`hist_char2type`] - always resolves to [`HistoryType::Search`]
/// today, since `get_cmdline_firstc()` is always `0` (NUL) and
/// `hist_char2type(0)` is `Search`.
#[must_use]
pub fn get_histtype(name: &[u8], return_default: bool) -> HistoryType {
    if name.is_empty() {
        return if return_default {
            HistoryType::Default
        } else {
            hist_char2type(crate::ex_getln::get_cmdline_firstc())
        };
    }

    for (hn, ty) in HISTORY_NAMES.iter().zip(HISTORY_NAME_TYPES.iter()) {
        // `STRNICMP(name, hn, name.len())`: a case-insensitive
        // comparison of `name`'s own full length against `hn`'s FIRST
        // `name.len()` bytes - only meaningful (and only ever matches)
        // when `name` is no longer than `hn` itself, since C's
        // `strncasecmp` would otherwise compare past `hn`'s own NUL
        // terminator against a byte that can never be NUL in `name`.
        if name.len() <= hn.len() && name.eq_ignore_ascii_case(&hn[..name.len()]) {
            return *ty;
        }
    }

    if name.len() == 1 && b":=@>?/".contains(&name[0]) {
        return hist_char2type(i32::from(name[0]));
    }

    HistoryType::Invalid
}

/// Iterate over the history ring of `history_type`, oldest entry
/// first (`hist_iter`).
///
/// Pass `None` as `iter` to start; the returned `Option<usize>` is
/// the state to pass on the next call, and `None` means iteration is
/// finished. When `zero` is set, each entry is CLEARED as it is read
/// (used while reading a ShaDa file, which takes ownership).
///
/// The original walks raw `histentry_T *` pointers around the ring
/// and hands the caller an opaque `const void *`; an index into the
/// ring is used here instead, which needs no sentinel arithmetic and
/// cannot outlive a reallocation of the table.
///
/// The returned entry has `hisstr == None` when there was nothing to
/// read - the original signals the same thing by clearing `*hist`
/// up front, and its own caller (`shada_hist_iter`) tests exactly
/// that field.
///
/// # Safety
/// Must not run concurrently with any other access to the history
/// tables.
#[must_use]
pub unsafe fn hist_iter(
    iter: Option<usize>,
    history_type: HistoryType,
    zero: bool,
) -> (HistentryT, Option<usize>) {
    let t = history_type as usize;
    // SAFETY: forwarded from this function's own safety doc.
    let hisidx = (unsafe { HISIDX.get_mut() })[t];
    if hisidx == -1 {
        return (HistentryT::default(), None);
    }

    let hislen = get_hislen();
    // SAFETY: forwarded from this function's own safety doc.
    let table = &mut (unsafe { HISTORY.get_mut() })[t];
    // The original's hstart/hlast/hend are indices 0, hisidx and
    // hislen - 1 respectively.
    let hlast = hisidx as usize;
    let hend = (hislen - 1) as usize;
    if hlast >= table.len() || hend >= table.len() {
        return (HistentryT::default(), None);
    }

    let hiter = match iter {
        Some(i) => i,
        None => {
            // Scan forward from the newest entry for the oldest
            // occupied slot, wrapping at the end of the ring. If the
            // ring holds nothing else, this lands back on hlast.
            let mut hfirst = hlast;
            loop {
                hfirst += 1;
                if hfirst > hend {
                    hfirst = 0;
                }
                if table[hfirst].hisstr.is_some() || hfirst == hlast {
                    break;
                }
            }
            hfirst
        }
    };
    if hiter >= table.len() {
        return (HistentryT::default(), None);
    }

    let hist = table[hiter].clone();
    if zero {
        clear_hist_entry(&mut table[hiter]);
    }
    if hiter == hlast {
        return (hist, None);
    }
    let next = if hiter + 1 > hend { 0 } else { hiter + 1 };
    (hist, Some(next))
}

/// Gets the identifier of the newest entry in history table `histype`
/// (`get_history_idx`), or `-1` when there is no such entry.
///
/// # Safety
/// Must not run concurrently with any other access to the history
/// tables.
#[must_use]
pub unsafe fn get_history_idx(histype: HistoryType) -> i32 {
    if get_hislen() == 0 || (histype as i32) < 0 || (histype as i32) >= HIST_COUNT as i32 {
        return -1;
    }
    let t = histype as usize;
    // SAFETY: forwarded from this function's own safety doc.
    let idx = (unsafe { HISIDX.get_mut() })[t];
    if idx < 0 {
        return -1;
    }
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { HISTORY.get_mut() })[t]
        .get(idx as usize)
        .map_or(-1, |e| e.hisnum)
}

/// `histadd({history}, {item})` - add `{item}` to the given history
/// (`f_histadd`, `cmdhist.c`). Returns `1` on success, `0` otherwise.
///
/// Safe like its `f_histget`/`f_histnr` siblings so it can go in
/// `funcs.rs`'s own builtin table, which holds plain `fn` pointers;
/// the history tables' exclusive-access requirement is discharged
/// internally, since Vimscript evaluation is single-threaded.
pub fn f_histadd(
    argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    rettv.value = crate::eval::typval_defs::TypvalValue::Number(0);
    // SAFETY: single-threaded Vimscript evaluation - see this
    // function's own doc comment.
    if unsafe { crate::ex_cmds::check_secure() } {
        return;
    }
    let histype = match crate::eval::typval::tv_get_string_chk(&argvars[0]) {
        Some(name) => get_histtype(&name, false),
        None => HistoryType::Invalid,
    };
    if histype == HistoryType::Invalid {
        return;
    }

    // The original's `tv_get_string_buf` differs from `tv_get_string`
    // only in taking a caller-supplied scratch buffer for the
    // number-to-string conversion; an owned return needs none.
    let str = match argvars.get(1) {
        Some(a) => crate::eval::typval::tv_get_string(a),
        None => Vec::new(),
    };
    if str.first().copied().unwrap_or(0) == 0 {
        return;
    }

    // SAFETY: as above.
    unsafe { init_history() };
    // SAFETY: as above.
    unsafe { add_to_history(histype, &str, false, 0) };
    rettv.value = crate::eval::typval_defs::TypvalValue::Number(1);
}

/// `histnr({history})` - the identifier of the newest entry in the
/// given history table, or `-1` for an unknown history name
/// (`f_histnr`, `cmdhist.c`).
///
/// Safe for the same reason as [`f_histadd`].
pub fn f_histnr(
    argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    let histname = crate::eval::typval::tv_get_string_chk(&argvars[0]);
    let i = match &histname {
        Some(name) => get_histtype(name, false),
        None => HistoryType::Invalid,
    };
    let n = if i == HistoryType::Invalid {
        HistoryType::Invalid as i32
    } else {
        // SAFETY: single-threaded Vimscript evaluation - see this
        // function's own doc comment.
        unsafe { get_history_idx(i) }
    };
    rettv.value = crate::eval::typval_defs::TypvalValue::Number(i64::from(n));
}

/// Calculate an entry's index in the history array for a given
/// history number (`calc_hist_idx`).
///
/// A positive `num` is an absolute entry number, searched backwards
/// from the newest entry (wrapping the ring exactly once). A negative
/// `num` counts backwards from the newest entry, where `-1` is the
/// newest itself. Returns `-1` when there is no such entry.
///
/// # Safety
/// Must not run concurrently with any other access to the history
/// tables.
unsafe fn calc_hist_idx(histype: HistoryType, num: i32) -> i32 {
    let hislen = get_hislen();
    if hislen == 0
        || (histype as i32) < 0
        || (histype as i32) >= HIST_COUNT as i32
        || num == 0
    {
        return -1;
    }
    let t = histype as usize;
    // SAFETY: forwarded from this function's own safety doc.
    let mut i = (unsafe { HISIDX.get_mut() })[t];
    if i < 0 {
        return -1;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let hist = &(unsafe { HISTORY.get_mut() })[t];
    let at = |i: i32| hist.get(i as usize);

    if num > 0 {
        let mut wrapped = false;
        while at(i).is_some_and(|e| e.hisnum > num) {
            i -= 1;
            if i < 0 {
                if wrapped {
                    break;
                }
                i += hislen;
                wrapped = true;
            }
        }
        if i >= 0
            && at(i).is_some_and(|e| e.hisnum == num && e.hisstr.is_some())
        {
            return i;
        }
    } else if -num <= hislen {
        i += num + 1;
        if i < 0 {
            i += hislen;
        }
        if at(i).is_some_and(|e| e.hisstr.is_some()) {
            return i;
        }
    }
    -1
}

/// Last seen `maptick` (`last_maptick`, a file-static in the
/// original - kept file-local here too).
static LAST_MAPTICK: GlobalCell<i32> = GlobalCell::new(-1);

/// Delete the history entry at index `idx` for `histype`
/// (`del_history_idx`). Returns whether an entry was removed.
///
/// # Safety
/// Must not run concurrently with any other access to the history
/// tables or to `GLOBALS.maptick`.
pub unsafe fn del_history_idx(histype: HistoryType, idx: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let mut i = unsafe { calc_hist_idx(histype, idx) };
    if i < 0 {
        return false;
    }
    let t = histype as usize;
    let hislen = get_hislen();
    // SAFETY: forwarded from this function's own safety doc.
    let idx = (unsafe { HISIDX.get_mut() })[t];
    // SAFETY: forwarded from this function's own safety doc.
    let history = unsafe { HISTORY.get_mut() };
    hist_free_entry(&mut history[t][i as usize]);

    // When deleting the last added search string in a mapping, reset
    // last_maptick, so that the last added search string isn't
    // deleted again.
    // SAFETY: forwarded from this function's own safety doc.
    let maptick = unsafe { crate::globals::GLOBALS.get_mut() }.maptick;
    // SAFETY: forwarded from this function's own safety doc.
    let last_maptick = unsafe { LAST_MAPTICK.get_mut() };
    if histype == HistoryType::Search && maptick == *last_maptick && i == idx {
        *last_maptick = -1;
    }

    while i != idx {
        let j = (i + 1) % hislen;
        history[t][i as usize] = history[t][j as usize].clone();
        i = j;
    }
    clear_hist_entry(&mut history[t][idx as usize]);
    i -= 1;
    if i < 0 {
        i += hislen;
    }
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { HISIDX.get_mut() })[t] = i;
    true
}

/// `histget({history} [, {index}])` - an entry from the given
/// command-line history, or an empty string if there is no such entry
/// (`f_histget`, `cmdhist.c`).
///
/// Only `hisstrlen` bytes of the stored entry are returned, NOT the
/// whole buffer: a search history entry carries its separator
/// character after the entry's own NUL, which must not leak into the
/// returned string.
///
/// A type-error on `{history}` itself (`tv_get_string_chk` returning
/// `None`) resolves to a `None` (null) string, matching the original's
/// own `rettv->vval.v_string = NULL` for that specific case.
pub fn f_histget(
    argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    let histname = crate::eval::typval::tv_get_string_chk(&argvars[0]);
    rettv.value = crate::eval::typval_defs::TypvalValue::String(match histname {
        None => None,
        Some(name) => {
            let histype = get_histtype(&name, false);
            let idx = if argvars.len() > 1 {
                crate::eval::typval::tv_get_number_chk(&argvars[1], None) as i32
            } else {
                // SAFETY: exclusive access to the history tables -
                // `f_histget` is only reached from single-threaded
                // Vimscript evaluation.
                unsafe { get_history_idx(histype) }
            };
            // SAFETY: exclusive access to the history tables, as
            // required by `calc_hist_idx` - `f_histget` is only
            // reached from single-threaded Vimscript evaluation.
            let i = unsafe { calc_hist_idx(histype, idx) };
            if i < 0 {
                Some(Vec::new())
            } else {
                // Note the original copies only `hisstrlen` bytes,
                // NOT the whole stored buffer: a search entry keeps
                // its separator character after the entry's own NUL,
                // and that must not leak into the returned string.
                // SAFETY: as above.
                let history = unsafe { HISTORY.get_mut() };
                let e = &history[histype as usize][i as usize];
                Some(
                    e.hisstr
                        .as_ref()
                        .map(|s| s[..e.hisstrlen.min(s.len())].to_vec())
                        .unwrap_or_default(),
                )
            }
        }
    });
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Test-only helper letting tests set the otherwise-private
    /// `HISLEN` value, matching the established `set_pum_is_visible`-
    /// style pattern (`popupmenu.rs`). Caller must hold
    /// `crate::globals::global_state_test_lock()` for the whole
    /// duration this value matters, and should restore the original
    /// value before releasing the lock.
    pub(crate) fn set_hislen(value: i32) -> i32 {
        let cell = unsafe { HISLEN.get_mut() };
        let old = *cell;
        *cell = value;
        old
    }

    /// Installs `entries` as history table `t` with `hisidx`/`hisnum`
    /// set, runs `f`, then restores everything. Caller must hold
    /// `global_state_test_lock()`.
    fn with_history<R>(
        t: HistoryType,
        entries: &[(&[u8], i32)],
        f: impl FnOnce() -> R,
    ) -> R {
        let idx = t as usize;
        let old_hislen = set_hislen(entries.len() as i32);
        let history = unsafe { HISTORY.get_mut() };
        let hisidx = unsafe { HISIDX.get_mut() };
        let hisnum = unsafe { HISNUM.get_mut() };
        let saved = (history[idx].clone(), hisidx[idx], hisnum[idx]);

        history[idx] = entries
            .iter()
            .enumerate()
            .map(|(i, (s, sep))| {
                // The original stores the search separator one byte
                // PAST the entry's own NUL, so the stored buffer is
                // "text\0<sep>".
                let mut buf = (*s).to_vec();
                buf.push(0);
                buf.push(u8::try_from(*sep).unwrap_or(0));
                HistentryT {
                    hisnum: i as i32 + 1,
                    hisstrlen: s.len(),
                    hisstr: Some(buf),
                    ..Default::default()
                }
            })
            .collect();
        hisidx[idx] = entries.len() as i32 - 1;
        hisnum[idx] = entries.len() as i32;

        let result = f();

        let history = unsafe { HISTORY.get_mut() };
        let hisidx = unsafe { HISIDX.get_mut() };
        let hisnum = unsafe { HISNUM.get_mut() };
        (history[idx], hisidx[idx], hisnum[idx]) = saved;
        set_hislen(old_hislen);
        result
    }

    // --- hist_iter ---

    /// Collects a whole iteration into a list of entry texts, exactly
    /// as `shada.c`'s own `hms_insert_whole_neovim_history` loop
    /// drives it.
    fn collect_hist_iter(t: HistoryType, zero: bool) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let mut iter = None;
        loop {
            let (entry, next) = unsafe { hist_iter(iter, t, zero) };
            if let Some(s) = entry.hisstr {
                out.push(s[..entry.hisstrlen].to_vec());
            }
            match next {
                Some(n) => iter = Some(n),
                None => break,
            }
        }
        out
    }

    #[test]
    fn hist_iter_reports_nothing_for_an_empty_table() {
        let _lock = crate::globals::global_state_test_lock();
        let (entry, next) = unsafe { hist_iter(None, HistoryType::Cmd, false) };
        assert!(entry.hisstr.is_none());
        assert_eq!(next, None);
    }

    /// A full ring is walked oldest-first and ENDS on the newest
    /// entry (`hisidx`), which is the last one yielded.
    #[test]
    fn hist_iter_walks_a_full_ring_oldest_entry_first() {
        let _lock = crate::globals::global_state_test_lock();
        with_history(
            HistoryType::Cmd,
            &[(b"one", 0), (b"two", 0), (b"three", 0)],
            || {
                // hisidx is 2 ("three"), so iteration starts by
                // wrapping past the end to index 0 and finishes on
                // index 2.
                assert_eq!(
                    collect_hist_iter(HistoryType::Cmd, false),
                    vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()]
                );
            },
        );
    }

    /// With the newest entry NOT at the end of the ring, iteration
    /// must wrap: the entries after `hisidx` are older and come
    /// first. An implementation that simply scanned 0..len would
    /// return them in the wrong order.
    #[test]
    fn hist_iter_wraps_around_the_ring_from_the_newest_entry() {
        let _lock = crate::globals::global_state_test_lock();
        with_history(
            HistoryType::Cmd,
            &[(b"c", 0), (b"d", 0), (b"a", 0), (b"b", 0)],
            || {
                // Move the newest entry to index 1, making indices
                // 2 and 3 the OLDER half of the ring.
                let hisidx = unsafe { HISIDX.get_mut() };
                hisidx[HistoryType::Cmd as usize] = 1;
                assert_eq!(
                    collect_hist_iter(HistoryType::Cmd, false),
                    vec![
                        b"a".to_vec(),
                        b"b".to_vec(),
                        b"c".to_vec(),
                        b"d".to_vec()
                    ]
                );
            },
        );
    }

    /// Empty slots ahead of the newest entry are skipped by the
    /// initial scan - a partly filled ring must not yield them.
    #[test]
    fn hist_iter_skips_the_unused_slots_of_a_partly_filled_ring() {
        let _lock = crate::globals::global_state_test_lock();
        with_history(HistoryType::Cmd, &[(b"x", 0), (b"y", 0), (b"z", 0)], || {
            // Clear the tail of the ring, leaving only indices 0..=1
            // occupied, with the newest at index 1.
            let table = unsafe { HISTORY.get_mut() };
            clear_hist_entry(&mut table[HistoryType::Cmd as usize][2]);
            let hisidx = unsafe { HISIDX.get_mut() };
            hisidx[HistoryType::Cmd as usize] = 1;
            assert_eq!(
                collect_hist_iter(HistoryType::Cmd, false),
                vec![b"x".to_vec(), b"y".to_vec()]
            );
        });
    }

    /// `zero` empties each entry as it is read - this is what lets
    /// the ShaDa reader take ownership of the strings.
    #[test]
    fn hist_iter_with_zero_clears_each_entry_as_it_is_read() {
        let _lock = crate::globals::global_state_test_lock();
        with_history(HistoryType::Cmd, &[(b"one", 0), (b"two", 0)], || {
            assert_eq!(
                collect_hist_iter(HistoryType::Cmd, true),
                vec![b"one".to_vec(), b"two".to_vec()]
            );
            // Everything has been consumed.
            let table = unsafe { HISTORY.get_mut() };
            for entry in &table[HistoryType::Cmd as usize] {
                assert!(entry.hisstr.is_none());
            }
        });
    }

    /// Without `zero` the table must be left untouched.
    #[test]
    fn hist_iter_without_zero_leaves_the_table_intact() {
        let _lock = crate::globals::global_state_test_lock();
        with_history(HistoryType::Cmd, &[(b"one", 0), (b"two", 0)], || {
            let _ = collect_hist_iter(HistoryType::Cmd, false);
            let table = unsafe { HISTORY.get_mut() };
            assert_eq!(
                table[HistoryType::Cmd as usize]
                    .iter()
                    .filter(|e| e.hisstr.is_some())
                    .count(),
                2
            );
        });
    }

    #[test]
    fn clear_hist_entry_zeroes_every_field() {
        let mut e = HistentryT {
            hisnum: 7,
            hisstr: Some(b"x".to_vec()),
            hisstrlen: 1,
            timestamp: 123,
            additional_data: None,
        };
        clear_hist_entry(&mut e);
        assert_eq!(e.hisnum, 0);
        assert!(e.hisstr.is_none());
        assert_eq!(e.hisstrlen, 0);
        assert_eq!(e.timestamp, 0);
    }

    #[test]
    fn hist_free_entry_also_releases_the_entry_text() {
        let mut e = HistentryT {
            hisnum: 3,
            hisstr: Some(b"abc".to_vec()),
            hisstrlen: 3,
            ..Default::default()
        };
        hist_free_entry(&mut e);
        assert!(e.hisstr.is_none());
        assert_eq!(e.hisnum, 0);
    }

    #[test]
    fn in_history_is_false_for_an_empty_table() {
        let _lock = crate::globals::global_state_test_lock();
        // hisidx is -1 by default, so nothing can match.
        assert!(!unsafe { in_history(HistoryType::Cmd, b"x", false, 0) });
    }

    #[test]
    fn in_history_finds_an_existing_entry() {
        let _lock = crate::globals::global_state_test_lock();
        with_history(HistoryType::Cmd, &[(b"one", 0), (b"two", 0)], || {
            assert!(unsafe { in_history(HistoryType::Cmd, b"two\0\0", false, 0) });
            assert!(unsafe { in_history(HistoryType::Cmd, b"one\0\0", false, 0) });
            assert!(!unsafe { in_history(HistoryType::Cmd, b"three\0\0", false, 0) });
        });
    }

    #[test]
    fn in_history_requires_a_matching_separator_for_search_history() {
        let _lock = crate::globals::global_state_test_lock();
        // Entry stored with separator '/'; only that separator
        // matches, because HIST_SEARCH checks the byte one PAST the
        // entry's own NUL.
        with_history(HistoryType::Search, &[(b"pat", i32::from(b'/'))], || {
            let stored: &[u8] = b"pat\0/";
            assert!(unsafe {
                in_history(HistoryType::Search, stored, false, i32::from(b'/'))
            });
            assert!(!unsafe {
                in_history(HistoryType::Search, stored, false, i32::from(b'?'))
            });
        });
    }

    #[test]
    fn in_history_move_to_front_rotates_the_entry_and_renumbers_it() {
        let _lock = crate::globals::global_state_test_lock();
        with_history(HistoryType::Cmd, &[(b"one", 0), (b"two", 0)], || {
            // Moving the OLDER entry to the front rotates the ring.
            assert!(unsafe { in_history(HistoryType::Cmd, b"one\0\0", true, 0) });
            let history = unsafe { HISTORY.get_mut() };
            let idx = HistoryType::Cmd as usize;
            let last = unsafe { HISIDX.get_mut() }[idx] as usize;
            assert_eq!(
                history[idx][last].hisstr.as_deref(),
                Some(&b"one\0\0"[..])
            );
            // It gets a fresh, higher entry number.
            assert_eq!(history[idx][last].hisnum, 3);
        });
    }

    #[test]
    fn get_hislen_is_zero_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(get_hislen(), 0);
    }

    #[test]
    fn get_hislen_reflects_the_underlying_value() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_hislen(42);
        assert_eq!(get_hislen(), 42);
        set_hislen(old);
    }

    #[test]
    fn hist_char2type_maps_every_known_character() {
        assert_eq!(hist_char2type(i32::from(b':')), HistoryType::Cmd);
        assert_eq!(hist_char2type(i32::from(b'=')), HistoryType::Expr);
        assert_eq!(hist_char2type(i32::from(b'@')), HistoryType::Input);
        assert_eq!(hist_char2type(i32::from(b'>')), HistoryType::Debug);
        assert_eq!(hist_char2type(i32::from(b'/')), HistoryType::Search);
        assert_eq!(hist_char2type(i32::from(b'?')), HistoryType::Search);
        assert_eq!(hist_char2type(0), HistoryType::Search);
    }

    #[test]
    fn hist_char2type_unknown_char_is_invalid() {
        assert_eq!(hist_char2type(i32::from(b'x')), HistoryType::Invalid);
        assert_eq!(hist_char2type(-1), HistoryType::Invalid);
    }

    // --- get_histtype ---

    #[test]
    fn get_histtype_matches_full_names_case_insensitively() {
        assert_eq!(get_histtype(b"cmd", false), HistoryType::Cmd);
        assert_eq!(get_histtype(b"CMD", false), HistoryType::Cmd);
        assert_eq!(get_histtype(b"search", false), HistoryType::Search);
        assert_eq!(get_histtype(b"expr", false), HistoryType::Expr);
        assert_eq!(get_histtype(b"input", false), HistoryType::Input);
        assert_eq!(get_histtype(b"debug", false), HistoryType::Debug);
    }

    #[test]
    fn get_histtype_matches_a_significant_prefix() {
        // "It is sufficient to give the significant prefix of a
        // history name" (the original's own comment on
        // `history_names[]`).
        assert_eq!(get_histtype(b"s", false), HistoryType::Search);
        assert_eq!(get_histtype(b"sea", false), HistoryType::Search);
        assert_eq!(get_histtype(b"c", false), HistoryType::Cmd);
    }

    #[test]
    fn get_histtype_a_name_longer_than_any_table_entry_does_not_match() {
        // "searching" is longer than "search" itself - STRNICMP would
        // compare past "search"'s own NUL terminator and fail, so this
        // must NOT match Search (or anything else).
        assert_eq!(get_histtype(b"searching", false), HistoryType::Invalid);
    }

    #[test]
    fn get_histtype_matches_a_single_marker_character() {
        assert_eq!(get_histtype(b":", false), HistoryType::Cmd);
        assert_eq!(get_histtype(b"=", false), HistoryType::Expr);
        assert_eq!(get_histtype(b"@", false), HistoryType::Input);
        assert_eq!(get_histtype(b">", false), HistoryType::Debug);
        assert_eq!(get_histtype(b"/", false), HistoryType::Search);
        assert_eq!(get_histtype(b"?", false), HistoryType::Search);
    }

    #[test]
    fn get_histtype_unrecognized_name_is_invalid() {
        assert_eq!(get_histtype(b"bogus", false), HistoryType::Invalid);
        assert_eq!(get_histtype(b"x", false), HistoryType::Invalid);
    }

    #[test]
    fn get_histtype_empty_name_resolves_via_cmdline_firstc() {
        let _lock = crate::globals::global_state_test_lock();
        // get_cmdline_firstc() is always 0 (NUL) today, and
        // hist_char2type(0) is Search.
        assert_eq!(get_histtype(b"", false), HistoryType::Search);
    }

    #[test]
    fn get_histtype_empty_name_with_return_default_is_default() {
        assert_eq!(get_histtype(b"", true), HistoryType::Default);
    }

    // --- get_history_idx / f_histnr ---

    #[test]
    fn get_history_arg_lists_short_names_then_long_names_then_all() {
        // Order comes from the original: the six short names first,
        // then HISTORY_NAMES, then "all". Real nvim's own
        // `getcompletion('', 'history')` sorts its output, so it
        // confirms the SET (`/ : = > ? @ all cmd debug expr input
        // search` - twelve candidates) rather than this order.
        let got: Vec<Vec<u8>> = (0..12).filter_map(get_history_arg).collect();
        assert_eq!(got.len(), 12);
        assert_eq!(&got[..6], &[b":".to_vec(), b"=".to_vec(), b"@".to_vec(), b">".to_vec(), b"?".to_vec(), b"/".to_vec()]);
        assert_eq!(got[6], b"cmd".to_vec());
        assert_eq!(got[10], b"debug".to_vec());
        assert_eq!(got[11], b"all".to_vec());

        // Sorting our candidates reproduces real nvim's own list.
        let mut sorted = got.clone();
        sorted.sort();
        let joined: Vec<String> = sorted
            .iter()
            .map(|s| std::string::String::from_utf8_lossy(s).into_owned())
            .collect();
        assert_eq!(joined.join(" "), "/ : = > ? @ all cmd debug expr input search");
    }

    #[test]
    fn get_history_arg_is_none_past_the_end() {
        assert!(get_history_arg(12).is_none());
        assert!(get_history_arg(99).is_none());
        assert!(get_history_arg(-1).is_none());
    }

    #[test]
    fn histadd_stores_an_entry_and_reports_success() {
        let _lock = crate::globals::global_state_test_lock();
        let saved = save_history_state();
        let saved_num = *unsafe { HISNUM.get_mut() };

        with_p_hi(4, || {
            let mut rettv = crate::eval::typval_defs::TypvalT::default();
            f_histadd(
                &[
                    crate::eval::typval_defs::TypvalT {
                        value: crate::eval::typval_defs::TypvalValue::String(Some(
                            b"cmd".to_vec(),
                        )),
                        ..Default::default()
                    },
                    crate::eval::typval_defs::TypvalT {
                        value: crate::eval::typval_defs::TypvalValue::String(Some(
                            b"echo".to_vec(),
                        )),
                        ..Default::default()
                    },
                ],
                &mut rettv,
            );
            assert_eq!(
                rettv.value,
                crate::eval::typval_defs::TypvalValue::Number(1)
            );
            // The entry really landed in the :cmd table.
            assert_eq!(unsafe { get_history_idx(HistoryType::Cmd) }, 1);
        });

        *unsafe { HISNUM.get_mut() } = saved_num;
        restore_history_state(saved);
    }

    #[test]
    fn histadd_reports_failure_for_a_bad_name_or_an_empty_item() {
        let _lock = crate::globals::global_state_test_lock();
        let saved = save_history_state();
        let saved_num = *unsafe { HISNUM.get_mut() };

        with_p_hi(4, || {
            for (name, item) in [(&b"bogus"[..], &b"x"[..]), (b"cmd", b"")] {
                let mut rettv = crate::eval::typval_defs::TypvalT::default();
                f_histadd(
                    &[
                        crate::eval::typval_defs::TypvalT {
                            value: crate::eval::typval_defs::TypvalValue::String(Some(
                                name.to_vec(),
                            )),
                            ..Default::default()
                        },
                        crate::eval::typval_defs::TypvalT {
                            value: crate::eval::typval_defs::TypvalValue::String(Some(
                                item.to_vec(),
                            )),
                            ..Default::default()
                        },
                    ],
                    &mut rettv,
                );
                assert_eq!(
                    rettv.value,
                    crate::eval::typval_defs::TypvalValue::Number(0),
                    "{:?}/{:?}",
                    std::string::String::from_utf8_lossy(name),
                    std::string::String::from_utf8_lossy(item)
                );
            }
        });

        *unsafe { HISNUM.get_mut() } = saved_num;
        restore_history_state(saved);
    }

    #[test]
    fn get_history_idx_is_negative_one_when_hislen_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(get_hislen(), 0);
        assert_eq!(unsafe { get_history_idx(HistoryType::Cmd) }, -1);
        assert_eq!(unsafe { get_history_idx(HistoryType::Search) }, -1);
    }

    #[test]
    fn get_history_idx_is_negative_one_for_an_out_of_range_type() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { get_history_idx(HistoryType::Invalid) }, -1);
        assert_eq!(unsafe { get_history_idx(HistoryType::Default) }, -1);
    }

    #[test]
    fn get_history_idx_reports_the_newest_entry_number() {
        let _lock = crate::globals::global_state_test_lock();
        with_history(HistoryType::Cmd, &[(b"one", 0), (b"two", 0)], || {
            // with_history numbers entries 1..=N, so the newest is 2.
            assert_eq!(unsafe { get_history_idx(HistoryType::Cmd) }, 2);
        });
        // An empty table still reports -1 via the hisidx < 0 branch.
        with_sized_history(4, || {
            assert_eq!(unsafe { get_history_idx(HistoryType::Cmd) }, -1);
        });
    }

    #[test]
    fn histnr_of_a_known_history_name_is_negative_one() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_histnr(
            &[crate::eval::typval_defs::TypvalT {
                value: crate::eval::typval_defs::TypvalValue::String(Some(b"cmd".to_vec())),
                ..Default::default()
            }],
            &mut rettv,
        );
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(-1));
    }

    #[test]
    fn histnr_of_an_unknown_history_name_is_also_negative_one() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_histnr(
            &[crate::eval::typval_defs::TypvalT {
                value: crate::eval::typval_defs::TypvalValue::String(Some(b"bogus".to_vec())),
                ..Default::default()
            }],
            &mut rettv,
        );
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(-1));
    }

    // --- calc_hist_idx / f_histget ---

    /// Runs `f` with `'history'` set to `newlen`, restoring it after.
    fn with_p_hi<R>(newlen: i64, f: impl FnOnce() -> R) -> R {
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let old = opts.p_hi;
        opts.p_hi = newlen;
        let result = f();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hi = old;
        result
    }

    /// Restores the history tables and `HISLEN` after a test that
    /// calls `init_history` directly.
    fn restore_history_state(saved: ([Vec<HistentryT>; HIST_COUNT], [i32; HIST_COUNT], i32)) {
        let (hist, idx, len) = saved;
        *unsafe { HISTORY.get_mut() } = hist;
        *unsafe { HISIDX.get_mut() } = idx;
        set_hislen(len);
    }

    fn save_history_state() -> ([Vec<HistentryT>; HIST_COUNT], [i32; HIST_COUNT], i32) {
        (
            unsafe { HISTORY.get_mut() }.clone(),
            *unsafe { HISIDX.get_mut() },
            get_hislen(),
        )
    }

    /// Sets up a sized, empty history table, runs `f`, restores.
    fn with_sized_history<R>(len: i32, f: impl FnOnce() -> R) -> R {
        let saved = save_history_state();
        let saved_num = *unsafe { HISNUM.get_mut() };
        with_p_hi(i64::from(len), || unsafe { init_history() });
        let result = f();
        *unsafe { HISNUM.get_mut() } = saved_num;
        restore_history_state(saved);
        result
    }

    #[test]
    fn add_to_history_stores_an_entry_with_its_separator_after_the_nul() {
        let _lock = crate::globals::global_state_test_lock();
        with_sized_history(4, || {
            unsafe { add_to_history(HistoryType::Search, b"pat", false, i32::from(b'/')) };
            let t = HistoryType::Search as usize;
            let idx = (unsafe { HISIDX.get_mut() })[t];
            assert_eq!(idx, 0);
            let e = &(unsafe { HISTORY.get_mut() })[t][idx as usize];
            assert_eq!(e.hisstrlen, 3);
            assert_eq!(e.hisnum, 1);
            // The buffer is `new_entrylen + 2` bytes: text, NUL, sep.
            assert_eq!(e.hisstr.as_deref(), Some(&b"pat\0/"[..]));
        });
    }

    #[test]
    fn add_to_history_advances_and_wraps_the_ring() {
        let _lock = crate::globals::global_state_test_lock();
        with_sized_history(2, || {
            let t = HistoryType::Cmd as usize;
            for s in [&b"one"[..], b"two", b"three"] {
                unsafe { add_to_history(HistoryType::Cmd, s, false, 0) };
            }
            // Three entries into a two-slot ring wraps back to slot 0.
            assert_eq!((unsafe { HISIDX.get_mut() })[t], 0);
            let h = &(unsafe { HISTORY.get_mut() })[t];
            assert_eq!(h[0].hisstr.as_deref(), Some(&b"three\0\0"[..]));
            // Entry numbers keep increasing across the wrap.
            assert_eq!(h[0].hisnum, 3);
        });
    }

    #[test]
    fn add_to_history_does_nothing_without_a_history_or_for_an_invalid_type() {
        let _lock = crate::globals::global_state_test_lock();
        // hislen is 0 by default.
        assert_eq!(get_hislen(), 0);
        unsafe { add_to_history(HistoryType::Cmd, b"x", false, 0) };
        assert!((unsafe { HISTORY.get_mut() })[HistoryType::Cmd as usize].is_empty());

        with_sized_history(2, || {
            unsafe { add_to_history(HistoryType::Invalid, b"x", false, 0) };
            // Nothing was stored anywhere.
            let h = unsafe { HISTORY.get_mut() };
            assert!(h.iter().all(|t| t.iter().all(|e| e.hisstr.is_none())));
        });
    }

    #[test]
    fn add_to_history_skips_search_history_under_keeppatterns() {
        let _lock = crate::globals::global_state_test_lock();
        with_sized_history(2, || {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let old = globals.cmdmod.cmod_flags;
            globals.cmdmod.cmod_flags |= crate::ex_cmds_defs::cmod::KEEPPATTERNS;

            unsafe { add_to_history(HistoryType::Search, b"pat", false, 0) };
            let s = HistoryType::Search as usize;
            assert_eq!((unsafe { HISIDX.get_mut() })[s], -1);

            // Only SEARCH history is skipped; :cmd history still adds.
            unsafe { add_to_history(HistoryType::Cmd, b"cmd", false, 0) };
            assert_eq!((unsafe { HISIDX.get_mut() })[HistoryType::Cmd as usize], 0);

            unsafe { crate::globals::GLOBALS.get_mut() }.cmdmod.cmod_flags = old;
        });
    }

    #[test]
    fn add_to_history_overwrites_consecutive_searches_from_one_mapping() {
        let _lock = crate::globals::global_state_test_lock();
        with_sized_history(4, || {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let old_tick = globals.maptick;
            globals.maptick = 7;

            let s = HistoryType::Search as usize;
            unsafe { add_to_history(HistoryType::Search, b"aa", true, 0) };
            assert_eq!((unsafe { HISIDX.get_mut() })[s], 0);

            // Same maptick, so the previous entry is replaced rather
            // than appended - the index stays put.
            unsafe { add_to_history(HistoryType::Search, b"bb", true, 0) };
            assert_eq!((unsafe { HISIDX.get_mut() })[s], 0);
            assert_eq!(
                (unsafe { HISTORY.get_mut() })[s][0].hisstr.as_deref(),
                Some(&b"bb\0\0"[..])
            );

            unsafe { crate::globals::GLOBALS.get_mut() }.maptick = old_tick;
        });
    }

    #[test]
    fn add_to_history_moves_a_duplicate_to_the_front_instead_of_appending() {
        let _lock = crate::globals::global_state_test_lock();
        with_sized_history(4, || {
            let t = HistoryType::Cmd as usize;
            unsafe { add_to_history(HistoryType::Cmd, b"one", false, 0) };
            unsafe { add_to_history(HistoryType::Cmd, b"two", false, 0) };
            assert_eq!((unsafe { HISIDX.get_mut() })[t], 1);

            // Re-adding an existing entry goes through in_history's
            // move-to-front path, so no new slot is consumed.
            unsafe { add_to_history(HistoryType::Cmd, b"one\0\0", false, 0) };
            assert_eq!((unsafe { HISIDX.get_mut() })[t], 1);
            assert_eq!(
                (unsafe { HISTORY.get_mut() })[t][1].hisstr.as_deref(),
                Some(&b"one\0\0"[..])
            );
        });
    }

    #[test]
    fn init_history_sizes_the_tables_from_the_history_option() {
        let _lock = crate::globals::global_state_test_lock();
        let saved = save_history_state();

        with_p_hi(4, || unsafe { init_history() });
        assert_eq!(get_hislen(), 4);
        for t in 0..HIST_COUNT {
            assert_eq!((unsafe { HISTORY.get_mut() })[t].len(), 4);
            // No entries carried over, so the index marks "empty".
            assert_eq!((unsafe { HISIDX.get_mut() })[t], -1);
        }

        restore_history_state(saved);
    }

    #[test]
    fn init_history_is_a_no_op_when_the_length_is_unchanged() {
        let _lock = crate::globals::global_state_test_lock();
        let saved = save_history_state();

        with_p_hi(3, || unsafe { init_history() });
        // Put a recognisable entry in place, then re-run with the
        // SAME length: the early return must leave it untouched.
        (unsafe { HISTORY.get_mut() })[0][0].hisnum = 42;
        (unsafe { HISIDX.get_mut() })[0] = 0;
        with_p_hi(3, || unsafe { init_history() });
        assert_eq!((unsafe { HISTORY.get_mut() })[0][0].hisnum, 42);
        assert_eq!((unsafe { HISIDX.get_mut() })[0], 0);

        restore_history_state(saved);
    }

    #[test]
    fn init_history_reorders_entries_oldest_first_when_growing() {
        let _lock = crate::globals::global_state_test_lock();
        let saved = save_history_state();

        // A two-slot ring holding "one","two" with hisidx at the last
        // slot; growing to four must carry both over, oldest first,
        // and leave hisidx pointing at the last copied entry.
        set_hislen(2);
        (unsafe { HISTORY.get_mut() })[0] = vec![
            HistentryT { hisnum: 1, hisstr: Some(b"one".to_vec()), hisstrlen: 3, ..Default::default() },
            HistentryT { hisnum: 2, hisstr: Some(b"two".to_vec()), hisstrlen: 3, ..Default::default() },
        ];
        (unsafe { HISIDX.get_mut() })[0] = 1;

        with_p_hi(4, || unsafe { init_history() });

        assert_eq!(get_hislen(), 4);
        let h = &(unsafe { HISTORY.get_mut() })[0];
        assert_eq!(h.len(), 4);
        assert_eq!(h[0].hisnum, 1);
        assert_eq!(h[1].hisnum, 2);
        // The grown tail is cleared.
        assert!(h[2].hisstr.is_none());
        assert!(h[3].hisstr.is_none());
        assert_eq!((unsafe { HISIDX.get_mut() })[0], 1);

        restore_history_state(saved);
    }

    #[test]
    fn init_history_drops_the_oldest_entries_when_shrinking() {
        let _lock = crate::globals::global_state_test_lock();
        let saved = save_history_state();

        // Four entries shrinking to two keeps only the NEWEST two.
        set_hislen(4);
        (unsafe { HISTORY.get_mut() })[0] = (1..=4)
            .map(|n| HistentryT {
                hisnum: n,
                hisstr: Some(vec![b'a' + n as u8]),
                hisstrlen: 1,
                ..Default::default()
            })
            .collect();
        (unsafe { HISIDX.get_mut() })[0] = 3;

        with_p_hi(2, || unsafe { init_history() });

        assert_eq!(get_hislen(), 2);
        let h = &(unsafe { HISTORY.get_mut() })[0];
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].hisnum, 3);
        assert_eq!(h[1].hisnum, 4);
        assert_eq!((unsafe { HISIDX.get_mut() })[0], 1);

        restore_history_state(saved);
    }

    #[test]
    fn init_history_clears_the_tables_when_history_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let saved = save_history_state();

        set_hislen(2);
        (unsafe { HISTORY.get_mut() })[0] = vec![HistentryT::default(); 2];
        (unsafe { HISIDX.get_mut() })[0] = 1;

        with_p_hi(0, || unsafe { init_history() });

        assert_eq!(get_hislen(), 0);
        assert!((unsafe { HISTORY.get_mut() })[0].is_empty());

        restore_history_state(saved);
    }

    #[test]
    fn history_table_accessors_round_trip() {
        let _lock = crate::globals::global_state_test_lock();
        with_history(HistoryType::Cmd, &[(b"one", 0), (b"two", 0)], || {
            // get_histentry sees what with_history installed.
            let entries = unsafe { get_histentry(HistoryType::Cmd) };
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].hisstrlen, 3);

            // hisidx/hisnum reflect the same setup.
            assert_eq!(unsafe { get_hisidx(HistoryType::Cmd) }, 1);
            assert_eq!(unsafe { get_hisnum(HistoryType::Cmd) }, 2);

            // The write halves of the original's own `int *` returns.
            unsafe { set_hisidx(HistoryType::Cmd, 0) };
            assert_eq!(unsafe { get_hisidx(HistoryType::Cmd) }, 0);
            unsafe { set_hisnum(HistoryType::Cmd, 7) };
            assert_eq!(unsafe { get_hisnum(HistoryType::Cmd) }, 7);

            unsafe { set_histentry(HistoryType::Cmd, Vec::new()) };
            assert!(unsafe { get_histentry(HistoryType::Cmd) }.is_empty());
        });
    }

    #[test]
    fn clr_history_empties_the_table_and_resets_its_counters() {
        let _lock = crate::globals::global_state_test_lock();
        with_history(HistoryType::Cmd, &[(b"one", 0), (b"two", 0)], || {
            assert_eq!(
                unsafe { clr_history(HistoryType::Cmd) },
                crate::vim_defs::OK
            );
            // Every entry is cleared, but the table keeps its length.
            let entries = unsafe { get_histentry(HistoryType::Cmd) };
            assert_eq!(entries.len(), 2);
            assert!(entries.iter().all(|e| e.hisstr.is_none()));
            // hisidx marks the history as cleared; hisnum resets.
            assert_eq!(unsafe { get_hisidx(HistoryType::Cmd) }, -1);
            assert_eq!(unsafe { get_hisnum(HistoryType::Cmd) }, 0);
        });
    }

    #[test]
    fn clr_history_fails_when_hislen_is_zero_or_the_type_is_invalid() {
        let _lock = crate::globals::global_state_test_lock();
        // hislen is 0 by default, so there is nothing to clear.
        assert_eq!(get_hislen(), 0);
        assert_eq!(
            unsafe { clr_history(HistoryType::Cmd) },
            crate::vim_defs::FAIL
        );
        // A non-real history type fails even with a sized table.
        with_history(HistoryType::Cmd, &[(b"one", 0)], || {
            assert_eq!(
                unsafe { clr_history(HistoryType::Invalid) },
                crate::vim_defs::FAIL
            );
        });
    }

    #[test]
    fn calc_hist_idx_is_negative_one_when_hislen_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(get_hislen(), 0);
        assert_eq!(unsafe { calc_hist_idx(HistoryType::Cmd, 1) }, -1);
        assert_eq!(unsafe { calc_hist_idx(HistoryType::Search, -1) }, -1);
    }

    #[test]
    fn calc_hist_idx_is_negative_one_for_an_out_of_range_type_or_zero_num() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { calc_hist_idx(HistoryType::Invalid, 1) }, -1);
        assert_eq!(unsafe { calc_hist_idx(HistoryType::Default, 1) }, -1);
        assert_eq!(unsafe { calc_hist_idx(HistoryType::Cmd, 0) }, -1);
    }

    #[test]
    fn calc_hist_idx_finds_an_entry_by_absolute_number() {
        let _lock = crate::globals::global_state_test_lock();
        // with_history numbers entries 1..=N in slot order.
        with_history(HistoryType::Cmd, &[(b"one", 0), (b"two", 0)], || {
            assert_eq!(unsafe { calc_hist_idx(HistoryType::Cmd, 1) }, 0);
            assert_eq!(unsafe { calc_hist_idx(HistoryType::Cmd, 2) }, 1);
            // A number with no matching entry.
            assert_eq!(unsafe { calc_hist_idx(HistoryType::Cmd, 99) }, -1);
        });
    }

    #[test]
    fn calc_hist_idx_counts_backwards_for_a_negative_number() {
        let _lock = crate::globals::global_state_test_lock();
        with_history(HistoryType::Cmd, &[(b"one", 0), (b"two", 0)], || {
            // -1 is the NEWEST entry, which is the last slot.
            assert_eq!(unsafe { calc_hist_idx(HistoryType::Cmd, -1) }, 1);
            assert_eq!(unsafe { calc_hist_idx(HistoryType::Cmd, -2) }, 0);
            // Further back than the table is long.
            assert_eq!(unsafe { calc_hist_idx(HistoryType::Cmd, -3) }, -1);
        });
    }

    #[test]
    fn histget_returns_a_real_entry_now_that_the_tables_exist() {
        let _lock = crate::globals::global_state_test_lock();
        with_history(HistoryType::Cmd, &[(b"one", 0), (b"two", 0)], || {
            let mut rettv = crate::eval::typval_defs::TypvalT::default();
            f_histget(
                &[
                    crate::eval::typval_defs::TypvalT {
                        value: crate::eval::typval_defs::TypvalValue::String(Some(
                            b"cmd".to_vec(),
                        )),
                        ..Default::default()
                    },
                    crate::eval::typval_defs::TypvalT {
                        value: crate::eval::typval_defs::TypvalValue::Number(-1),
                        ..Default::default()
                    },
                ],
                &mut rettv,
            );
            // Only `hisstrlen` bytes come back - the stored buffer
            // also carries a NUL and a separator byte after the text.
            assert_eq!(
                rettv.value,
                crate::eval::typval_defs::TypvalValue::String(Some(b"two".to_vec()))
            );
        });
    }

    #[test]
    fn del_history_idx_removes_an_entry_and_moves_the_index_back() {
        let _lock = crate::globals::global_state_test_lock();
        with_history(HistoryType::Cmd, &[(b"one", 0), (b"two", 0)], || {
            // Delete the newest entry.
            assert!(unsafe { del_history_idx(HistoryType::Cmd, -1) });
            let hisidx = unsafe { HISIDX.get_mut() }[HistoryType::Cmd as usize];
            assert_eq!(hisidx, 0);
            // The remaining entry is still reachable as the newest.
            assert_eq!(unsafe { calc_hist_idx(HistoryType::Cmd, -1) }, 0);
        });
    }

    #[test]
    fn del_history_idx_is_false_for_a_missing_entry() {
        let _lock = crate::globals::global_state_test_lock();
        // Empty table.
        assert!(!unsafe { del_history_idx(HistoryType::Cmd, -1) });
        with_history(HistoryType::Cmd, &[(b"one", 0)], || {
            assert!(!unsafe { del_history_idx(HistoryType::Cmd, 99) });
            assert!(!unsafe { del_history_idx(HistoryType::Cmd, 0) });
        });
    }

    #[test]
    fn histget_of_a_known_history_name_is_an_empty_string() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_histget(
            &[crate::eval::typval_defs::TypvalT {
                value: crate::eval::typval_defs::TypvalValue::String(Some(b"search".to_vec())),
                ..Default::default()
            }],
            &mut rettv,
        );
        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(Some(Vec::new()))
        );
    }

    #[test]
    fn histget_with_an_explicit_index_argument_is_also_an_empty_string() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_histget(
            &[
                crate::eval::typval_defs::TypvalT {
                    value: crate::eval::typval_defs::TypvalValue::String(Some(b"cmd".to_vec())),
                    ..Default::default()
                },
                crate::eval::typval_defs::TypvalT {
                    value: crate::eval::typval_defs::TypvalValue::Number(-2),
                    ..Default::default()
                },
            ],
            &mut rettv,
        );
        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(Some(Vec::new()))
        );
    }

    #[test]
    fn histget_of_a_type_error_history_name_is_a_null_string() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_histget(
            &[crate::eval::typval_defs::TypvalT {
                value: crate::eval::typval_defs::TypvalValue::List(std::ptr::null_mut()),
                ..Default::default()
            }],
            &mut rettv,
        );
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::String(None));
    }
}
