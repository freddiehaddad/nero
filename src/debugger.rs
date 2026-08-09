//! Translated from `src/nvim/debugger.c` (tractable core only).
//!
//! `debugger.c` implements the `:debug`/breakpoint (`:breakadd`/
//! `:breakdel`/`:breaklist`) command-line debugger. Most functions
//! still need `do_debug` (the real interactive debug-mode REPL,
//! reading commands via the not-yet-translated command-line input
//! subsystem) and/or real message display.
//!
//! Translated: `ex_debuggreedy` (the `:debuggreedy` Ex-command
//! handler - just flips the `debug_greedy` file-static based on
//! whether an address was given). No real caller yet (`do_debug`,
//! its only reader, isn't translated) - translated ahead of it
//! anyway, matching this crate's established "translate a small,
//! simple, mechanically-correct piece ahead of the surrounding
//! engine" precedent.
//!
//! Also translated: [`Debuggy`] (`struct debuggy`), the `dbg_breakp`
//! and `prof_ga` lists, the `DBG_*` type constants, and
//! [`update_has_expr_breakpoint`]/[`has_expr_breakpoint`]. The lists
//! are [`crate::garray_defs::TypedGarrayT`]s rather than the
//! original's byte-erased `garray_T`, since a `Debuggy` owns its
//! `dbg_name` and `dbg_val`.
//!
//! Deferred: everything else in the file - `dbg_parsearg`/
//! `ex_breakadd`/`ex_breakdel`/`ex_breaklist`/`dbg_find_breakpoint`
//! all need `vim_regcomp`/`file_pat_to_reg_pat` (the regex engine) or
//! real message display.

use crate::ex_cmds_defs::ExargT;
use crate::globals::GlobalCell;

/// One breakpoint or profiling entry (`struct debuggy`).
///
/// `dbg_prog` stays a raw pointer to the opaque
/// [`crate::types_defs::RegprogT`] placeholder, matching how every
/// other not-yet-translated regex program is carried in this crate
/// (e.g. `AutoPatT.reg_prog`, `SynblockT.b_syn_linecont_prog`).
#[derive(Debug)]
pub struct Debuggy {
    /// breakpoint number (`dbg_nr`).
    pub dbg_nr: i32,
    /// [`DBG_FUNC`], [`DBG_FILE`] or [`DBG_EXPR`] (`dbg_type`).
    pub dbg_type: i32,
    /// function, expression or file name (`dbg_name`).
    pub dbg_name: Option<Vec<u8>>,
    /// regexp program (`dbg_prog`).
    pub dbg_prog: *mut crate::types_defs::RegprogT,
    /// line number in function or file (`dbg_lnum`).
    pub dbg_lnum: crate::pos_defs::LinenrT,
    /// `!` used (`dbg_forceit`).
    pub dbg_forceit: i32,
    /// last result of a watchexpression (`dbg_val`).
    pub dbg_val: Option<Box<crate::eval::typval_defs::TypvalT>>,
    /// stored nested level for an expression breakpoint (`dbg_level`).
    pub dbg_level: i32,
}

impl Default for Debuggy {
    fn default() -> Self {
        Debuggy {
            dbg_nr: 0,
            dbg_type: 0,
            dbg_name: None,
            dbg_prog: std::ptr::null_mut(),
            dbg_lnum: 0,
            dbg_forceit: 0,
            dbg_val: None,
            dbg_level: 0,
        }
    }
}

/// Breakpoint on a function (`DBG_FUNC`).
pub const DBG_FUNC: i32 = 1;
/// Breakpoint on a sourced file (`DBG_FILE`).
pub const DBG_FILE: i32 = 2;
/// Breakpoint on an expression becoming true (`DBG_EXPR`).
pub const DBG_EXPR: i32 = 3;

/// The breakpoint list (`dbg_breakp`).
///
/// A [`crate::garray_defs::TypedGarrayT`] rather than the original's
/// byte-erased `garray_T`, because a [`Debuggy`] owns its `dbg_name`
/// and `dbg_val` - see `TypedGarrayT`'s own doc comment. The
/// original's grow size of 4 is preserved.
pub static DBG_BREAKP: std::sync::LazyLock<
    GlobalCell<crate::garray_defs::TypedGarrayT<Debuggy>>,
> = std::sync::LazyLock::new(|| GlobalCell::new(crate::garray_defs::TypedGarrayT::new(4)));

/// Profiling uses file and function names similar to breakpoints
/// (`prof_ga`).
pub static PROF_GA: std::sync::LazyLock<
    GlobalCell<crate::garray_defs::TypedGarrayT<Debuggy>>,
> = std::sync::LazyLock::new(|| GlobalCell::new(crate::garray_defs::TypedGarrayT::new(4)));

/// Whether any expression breakpoint is currently set
/// (`has_expr_breakpoint`).
static HAS_EXPR_BREAKPOINT: GlobalCell<bool> = GlobalCell::new(false);

/// Recompute the cached `has_expr_breakpoint` flag from the
/// breakpoint list (`update_has_expr_breakpoint`).
///
/// Cached because it is consulted on every executed line, while
/// breakpoints change rarely.
///
/// # Safety
/// Touches the `dbg_breakp` and `has_expr_breakpoint` file-statics.
pub unsafe fn update_has_expr_breakpoint() {
    // SAFETY: forwarded from this function's own safety doc.
    let any_expr = unsafe { DBG_BREAKP.get_mut() }
        .items
        .iter()
        .any(|bp| bp.dbg_type == DBG_EXPR);
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *HAS_EXPR_BREAKPOINT.get_mut() = any_expr };
}

/// Whether any expression breakpoint is currently set
/// (`has_expr_breakpoint`).
///
/// # Safety
/// Touches the `has_expr_breakpoint` file-static.
#[must_use]
pub unsafe fn has_expr_breakpoint() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *HAS_EXPR_BREAKPOINT.get_mut() }
}

/// Whether `:debug` mode reads input directly rather than via
/// `typeahead`, set by `:debuggreedy`/`:0debuggreedy`
/// (`debug_greedy`).
static DEBUG_GREEDY: GlobalCell<bool> = GlobalCell::new(false);

/// `":debuggreedy"` (`ex_debuggreedy`). With no address given, or an
/// explicit NONZERO address (e.g. `:5debuggreedy`), enables greedy
/// mode; an explicit address of exactly `0` (`:0debuggreedy`) is the
/// one combination that disables it - matching the original's own
/// real `eap->addr_count == 0 || eap->line2 != 0` condition exactly
/// (a real, deliberate upstream quirk: `:0debuggreedy` is NOT treated
/// the same as no address at all).
///
/// # Safety
/// Single-threaded test/editor state, matching every other
/// `GlobalCell`-backed file-static in this crate.
pub unsafe fn ex_debuggreedy(eap: &ExargT) {
    let greedy = eap.addr_count == 0 || eap.line2 != 0;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *DEBUG_GREEDY.get_mut() = greedy };
}

/// # Safety
/// Same as [`ex_debuggreedy`].
#[must_use]
pub unsafe fn debug_greedy() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *DEBUG_GREEDY.get_mut() }
}

/// Name of the script/function a breakpoint was just hit in
/// (`debug_breakpoint_name`). File-static in the original.
static DEBUG_BREAKPOINT_NAME: GlobalCell<Option<Vec<u8>>> = GlobalCell::new(None);

/// Line number of the breakpoint just hit (`debug_breakpoint_lnum`).
/// File-static in the original.
static DEBUG_BREAKPOINT_LNUM: GlobalCell<crate::pos_defs::LinenrT> = GlobalCell::new(0);

/// Record that a breakpoint was reached (`dbg_breakpoint`).
///
/// Only records it: the original's own comment notes the line still
/// has to be confirmed as actually executed, which `do_one_cmd` does
/// later by consulting these two values.
///
/// The original stores the caller's `char *` directly, borrowing it;
/// this owns a copy instead, since nothing here can guarantee the
/// caller's buffer outlives the record.
///
/// # Safety
/// Same as [`ex_debuggreedy`].
pub unsafe fn dbg_breakpoint(name: &[u8], lnum: crate::pos_defs::LinenrT) {
    // We need to check if this line is actually executed in do_one_cmd().
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        *DEBUG_BREAKPOINT_NAME.get_mut() = Some(name.to_vec());
        *DEBUG_BREAKPOINT_LNUM.get_mut() = lnum;
    }
}

/// The breakpoint name recorded by [`dbg_breakpoint`], if any.
///
/// # Safety
/// Same as [`ex_debuggreedy`].
#[must_use]
pub unsafe fn debug_breakpoint_name() -> Option<Vec<u8>> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { DEBUG_BREAKPOINT_NAME.get_mut() }.clone()
}

/// The breakpoint line recorded by [`dbg_breakpoint`].
///
/// # Safety
/// Same as [`ex_debuggreedy`].
#[must_use]
pub unsafe fn debug_breakpoint_lnum() -> crate::pos_defs::LinenrT {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *DEBUG_BREAKPOINT_LNUM.get_mut() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::global_state_test_lock;

    // --- dbg_breakp / update_has_expr_breakpoint ---

    /// Restores the breakpoint list and cached flag on drop, even
    /// through a panic.
    struct BreakpGuard {
        items: Vec<Debuggy>,
        flag: bool,
    }

    impl BreakpGuard {
        fn save() -> Self {
            let ga = unsafe { DBG_BREAKP.get_mut() };
            Self {
                items: std::mem::take(&mut ga.items),
                flag: unsafe { *HAS_EXPR_BREAKPOINT.get_mut() },
            }
        }
    }

    impl Drop for BreakpGuard {
        fn drop(&mut self) {
            let ga = unsafe { DBG_BREAKP.get_mut() };
            ga.items = std::mem::take(&mut self.items);
            unsafe { *HAS_EXPR_BREAKPOINT.get_mut() = self.flag };
        }
    }

    fn breakpoint(dbg_type: i32, name: &[u8]) -> Debuggy {
        Debuggy { dbg_type, dbg_name: Some(name.to_vec()), ..Debuggy::default() }
    }

    #[test]
    fn dbg_breakp_starts_empty_with_the_originals_grow_size() {
        let _lock = global_state_test_lock();
        let _g = BreakpGuard::save();
        let ga = unsafe { DBG_BREAKP.get_mut() };
        ga.items.clear();
        assert!(ga.is_empty());
        assert_eq!(ga.ga_growsize, 4);
    }

    #[test]
    fn has_expr_breakpoint_is_false_with_no_breakpoints() {
        let _lock = global_state_test_lock();
        let _g = BreakpGuard::save();
        unsafe { DBG_BREAKP.get_mut() }.items.clear();
        unsafe { update_has_expr_breakpoint() };
        assert!(!unsafe { has_expr_breakpoint() });
    }

    /// Only DBG_EXPR counts: function and file breakpoints are
    /// checked by name at call/source time, not re-evaluated per line.
    #[test]
    fn has_expr_breakpoint_ignores_func_and_file_breakpoints() {
        let _lock = global_state_test_lock();
        let _g = BreakpGuard::save();
        let ga = unsafe { DBG_BREAKP.get_mut() };
        ga.items = vec![
            breakpoint(DBG_FUNC, b"Foo"),
            breakpoint(DBG_FILE, b"bar.vim"),
        ];
        unsafe { update_has_expr_breakpoint() };
        assert!(!unsafe { has_expr_breakpoint() });
    }

    #[test]
    fn has_expr_breakpoint_is_true_when_one_is_present() {
        let _lock = global_state_test_lock();
        let _g = BreakpGuard::save();
        let ga = unsafe { DBG_BREAKP.get_mut() };
        // Deliberately NOT first, so a scan that only checked the
        // head of the list would miss it.
        ga.items = vec![
            breakpoint(DBG_FUNC, b"Foo"),
            breakpoint(DBG_EXPR, b"g:x > 1"),
        ];
        unsafe { update_has_expr_breakpoint() };
        assert!(unsafe { has_expr_breakpoint() });
    }

    /// The flag is RECOMPUTED, not merely set: removing the last
    /// expression breakpoint must clear it again. An implementation
    /// that only ever set the flag true would fail here.
    #[test]
    fn update_has_expr_breakpoint_clears_the_flag_when_the_last_one_goes() {
        let _lock = global_state_test_lock();
        let _g = BreakpGuard::save();
        let ga = unsafe { DBG_BREAKP.get_mut() };
        ga.items = vec![breakpoint(DBG_EXPR, b"g:x > 1")];
        unsafe { update_has_expr_breakpoint() };
        assert!(unsafe { has_expr_breakpoint() });

        unsafe { DBG_BREAKP.get_mut() }.items.clear();
        unsafe { update_has_expr_breakpoint() };
        assert!(!unsafe { has_expr_breakpoint() });
    }

    /// Clearing the list drops each entry's owned name - the whole
    /// reason the list is a TypedGarrayT rather than a byte-erased
    /// growarray.
    #[test]
    fn clearing_the_breakpoint_list_drops_the_owned_names() {
        let _lock = global_state_test_lock();
        let _g = BreakpGuard::save();
        let ga = unsafe { DBG_BREAKP.get_mut() };
        ga.items = vec![breakpoint(DBG_EXPR, b"g:x > 1")];
        assert_eq!(ga.ga_len(), 1);
        ga.ga_clear();
        assert_eq!(ga.ga_len(), 0);
        assert!(ga.is_empty());
    }

    #[test]
    fn dbg_type_constants_match_the_original() {
        assert_eq!((DBG_FUNC, DBG_FILE, DBG_EXPR), (1, 2, 3));
    }

    // --- dbg_breakpoint ---

    #[test]
    fn dbg_breakpoint_records_the_name_and_line() {
        let _lock = global_state_test_lock();
        unsafe {
            let prev_name = DEBUG_BREAKPOINT_NAME.get_mut().clone();
            let prev_lnum = *DEBUG_BREAKPOINT_LNUM.get_mut();

            dbg_breakpoint(b"function Foo", 42);

            assert_eq!(debug_breakpoint_name(), Some(b"function Foo".to_vec()));
            assert_eq!(debug_breakpoint_lnum(), 42);

            *DEBUG_BREAKPOINT_NAME.get_mut() = prev_name;
            *DEBUG_BREAKPOINT_LNUM.get_mut() = prev_lnum;
        }
    }

    #[test]
    fn dbg_breakpoint_overwrites_a_previous_record() {
        let _lock = global_state_test_lock();
        unsafe {
            let prev_name = DEBUG_BREAKPOINT_NAME.get_mut().clone();
            let prev_lnum = *DEBUG_BREAKPOINT_LNUM.get_mut();

            dbg_breakpoint(b"first", 1);
            dbg_breakpoint(b"second", 2);

            assert_eq!(debug_breakpoint_name(), Some(b"second".to_vec()));
            assert_eq!(debug_breakpoint_lnum(), 2);

            *DEBUG_BREAKPOINT_NAME.get_mut() = prev_name;
            *DEBUG_BREAKPOINT_LNUM.get_mut() = prev_lnum;
        }
    }

    #[test]
    fn dbg_breakpoint_owns_its_copy_of_the_name() {
        // The original borrows the caller's pointer; this owns a copy,
        // so mutating the caller's buffer afterwards cannot corrupt
        // the record.
        let _lock = global_state_test_lock();
        unsafe {
            let prev_name = DEBUG_BREAKPOINT_NAME.get_mut().clone();
            let prev_lnum = *DEBUG_BREAKPOINT_LNUM.get_mut();

            let mut name = b"script.vim".to_vec();
            dbg_breakpoint(&name, 7);
            name.clear();

            assert_eq!(debug_breakpoint_name(), Some(b"script.vim".to_vec()));

            *DEBUG_BREAKPOINT_NAME.get_mut() = prev_name;
            *DEBUG_BREAKPOINT_LNUM.get_mut() = prev_lnum;
        }
    }

    #[test]
    fn no_address_enables_greedy_mode() {
        let _lock = global_state_test_lock();
        let eap = ExargT { addr_count: 0, line2: 0, ..ExargT::default() };
        unsafe { ex_debuggreedy(&eap) };
        assert!(unsafe { debug_greedy() });
    }

    #[test]
    fn explicit_zero_address_disables_greedy_mode() {
        let _lock = global_state_test_lock();
        // `:0debuggreedy` - a real address was given (addr_count != 0)
        // AND it's exactly 0 (line2 == 0) - the one combination where
        // BOTH of ex_debuggreedy's own disjuncts are false, so this
        // explicitly turns greedy mode OFF (a real, deliberate
        // upstream quirk: an explicit ":0" is not the same as "no
        // address at all", verified by tracing the exact boolean
        // condition rather than assuming either "some address" or
        // "line2 == 0" alone determines the outcome).
        unsafe { *DEBUG_GREEDY.get_mut() = true };
        let eap = ExargT { addr_count: 1, line2: 0, ..ExargT::default() };
        unsafe { ex_debuggreedy(&eap) };
        assert!(!unsafe { debug_greedy() });
    }

    #[test]
    fn a_nonzero_address_enables_greedy_mode() {
        let _lock = global_state_test_lock();
        // `:5debuggreedy` - an address WAS given, but it's nonzero, so
        // the `line2 != 0` disjunct is true - greedy mode turns ON.
        let eap = ExargT { addr_count: 1, line2: 5, ..ExargT::default() };
        unsafe { ex_debuggreedy(&eap) };
        assert!(unsafe { debug_greedy() });
    }

    #[test]
    fn toggling_back_and_forth() {
        let _lock = global_state_test_lock();
        let on = ExargT { addr_count: 0, line2: 0, ..ExargT::default() };
        let off = ExargT { addr_count: 1, line2: 0, ..ExargT::default() };
        unsafe { ex_debuggreedy(&on) };
        assert!(unsafe { debug_greedy() });
        unsafe { ex_debuggreedy(&off) };
        assert!(!unsafe { debug_greedy() });
        unsafe { ex_debuggreedy(&on) };
        assert!(unsafe { debug_greedy() });
    }
}
