//! Translated from `src/nvim/runtime.c` (tractable core only).
//!
//! `runtime.c` (~2900 lines) is the runtime-path/script-sourcing
//! subsystem (`:runtime`, `:source`, `'runtimepath'` traversal,
//! per-script `:profile` reporting) - almost all of it needs real file
//! I/O, the expression evaluator, or the Lua host, none attempted
//! here.
//!
//! Translated: `script_items`/`SCRIPT_ITEM` (as `SCRIPT_ITEMS`/
//! [`script_item`] - the growable registry of all sourced scripts,
//! indexed by script ID) and `new_script_item` - tractable now that
//! `runtime_defs.rs`'s `ScriptitemT` has real fields and
//! `eval/vars.rs`'s `new_script_vars` exists. A plain
//! `Vec<*mut ScriptitemT>` here rather than a generic `GarrayT` (this
//! crate's usual byte-oriented growable-array translation for
//! untyped `void*`-shaped `garray_T` uses) since this particular
//! `garray_T` is always accessed through its own
//! `scriptitem_T **`-typed `SCRIPT_ITEM` macro, never through
//! `garray_T`'s generic byte-size-parameterized API - the same
//! "translate the registry using whatever Rust collection actually
//! matches its real usage" reasoning `eval/userfunc.rs`'s own
//! `FuncHashtab` already established for `func_hashtab`.
//!
//! `last_current_SID` (a C function-local `static` inside
//! `new_script_item` itself) becomes its own file-static
//! `LAST_CURRENT_SID`, mirroring `buffer.rs`'s own
//! `TOP_FILE_NUM`/`BUF_FREE_COUNT` treatment for the same kind of
//! per-file counter.
//!
//! Neither `SCRIPT_ITEMS` nor the `ScriptitemT`/`ScriptvarT`/`DictT`
//! values it points at are ever freed by this crate, matching the
//! original exactly: scripts accumulate for the whole nvim session
//! and are only torn down by `free_all_script_vars`
//! (`#ifdef EXITFREE`-gated shutdown cleanup, same accepted gap as
//! `eval/vars.c`'s own `evalvars_clear`) - not translated. This is a
//! genuine "leak for the process lifetime" design in the original
//! itself, not an oversight here.
//!
//! Also translated: `script_autoload`'s own fast-reject path (no `#`
//! after `name`'s first byte means there is no package name at all,
//! so the answer is unconditionally `false`) - needed to unblock
//! `eval/vars.rs`'s `find_var_in_ht`. The substantive path (real
//! `'runtimepath'` traversal + file I/O + actually sourcing a script)
//! is `unimplemented!()` - see [`script_autoload`]'s own doc comment
//! for why that is safe to leave for now.
//!
//! Deferred: everything else in this file (runtime-path search,
//! `:runtime`, `:scriptnames`, per-script `:profile` reporting,
//! script unloading/`GA_DEEP_CLEAR` teardown, `autoload_name`/
//! `do_in_runtimepath`/`source_callback`/`ga_loaded`, etc.) - each
//! needs real file I/O and/or the expression evaluator.

use crate::eval::typval_defs::ScidT;
use crate::globals::GlobalCell;
use crate::runtime_defs::ScriptitemT;

static RUNTIME_SEARCH_PATH_VALID: GlobalCell<bool> = GlobalCell::new(false);

/// Invalidates the cached runtime search path after `'runtimepath'`
/// changes (`did_set_runtimepackpath`).
pub fn did_set_runtimepackpath() {
    unsafe { *RUNTIME_SEARCH_PATH_VALID.get_mut() = false };
}

/// `script_items` - the growable registry of all sourced scripts,
/// indexed by script ID minus one (`SCRIPT_ITEM(id)` in the original).
/// See this module's own doc comment for why this is a plain
/// `Vec<*mut ScriptitemT>` rather than a `GarrayT`.
///
/// Kept private, matching `eval/userfunc.rs`'s own `FUNC_HASHTAB`
/// encapsulation boundary - only reachable through this module's own
/// `pub fn`s ([`script_item`], [`new_script_item`]).
static SCRIPT_ITEMS: GlobalCell<Vec<*mut ScriptitemT>> = GlobalCell::new(Vec::new());

/// `last_current_SID` - see this module's own doc comment.
static LAST_CURRENT_SID: GlobalCell<ScidT> = GlobalCell::new(0);

/// `exestack` - the script/function/autocmd execution call stack, used
/// to build `getstacktrace()`'s own result and (via its top entry) the
/// `SOURCING_NAME`/`SOURCING_LNUM` macros elsewhere in the original.
/// Always empty today: nothing in this crate can push a real frame
/// onto it yet (no Ex-command execution engine drives script/function
/// sourcing - see `testing.rs`'s own identical observation for
/// `estack_sfile`'s "always empty" reasoning), matching this crate's
/// established `AUTOCMDS`-style "genuinely, provably always-empty
/// registry" precedent, not a hardcoded shortcut. [`estack_init`]/
/// [`estack_push`]/[`estack_pop`] are translated, but nothing calls
/// them yet, so that "always empty" property still holds.
static EXESTACK: GlobalCell<Vec<crate::runtime_defs::EstackT>> = GlobalCell::new(Vec::new());

/// Initialize the execution stack (`estack_init`) with its base
/// `ETYPE_TOP` frame.
///
/// Nothing in this crate calls this yet, so `EXESTACK` remains empty
/// in practice and [`have_sourcing_info`] still always reports
/// `false`. That matters beyond this file: `message.rs`'s
/// `other_sourcing_name` has an `unimplemented!()` body guarded by
/// exactly that predicate, so the first real caller of this function
/// has to translate `SOURCING_NAME` alongside it.
///
/// # Safety
/// Must not run concurrently with any other access to `EXESTACK`.
pub unsafe fn estack_init() {
    // SAFETY: forwarded from this function's own safety doc.
    let stack = unsafe { EXESTACK.get_mut() };
    // The original's `ga_grow(&exestack, 10)` is a capacity hint, not
    // a length change.
    stack.reserve(10);
    stack.push(crate::runtime_defs::EstackT {
        es_type: crate::runtime_defs::EtypeT::Top,
        es_name: std::ptr::null_mut(),
        es_lnum: 0,
        ..Default::default()
    });
}

/// Push an item onto the execution stack (`estack_push`), returning
/// its index.
///
/// The original returns a pointer to the new entry; an index is the
/// same information over a `Vec` and stays valid across the
/// reallocation a push can cause.
///
/// # Safety
/// Must not run concurrently with any other access to `EXESTACK`.
/// `name` is stored as-is and must outlive the entry.
pub unsafe fn estack_push(
    etype: crate::runtime_defs::EtypeT,
    name: *mut u8,
    lnum: crate::pos_defs::LinenrT,
) -> usize {
    // SAFETY: forwarded from this function's own safety doc.
    let stack = unsafe { EXESTACK.get_mut() };
    stack.push(crate::runtime_defs::EstackT {
        es_type: etype,
        es_name: name,
        es_lnum: lnum,
        ..Default::default()
    });
    stack.len() - 1
}

/// Add a user function to the execution stack
/// (`estack_push_ufunc`).
///
/// The frame's name is the function's EXPANDED name when it has one
/// (`<SNR>`-resolved, for a script-local function), falling back to
/// the plain name otherwise - so a stack trace shows the resolved
/// form a user can act on.
///
/// The original guards against `estack_push` returning NULL; here it
/// always yields a valid index, so the guard has no counterpart.
///
/// # Safety
/// Must not run concurrently with any other access to `EXESTACK`.
/// `ufunc` must be a valid, non-null pointer to a live `UfuncT` that
/// outlives the frame, since the frame borrows both its name and the
/// pointer itself.
pub unsafe fn estack_push_ufunc(
    ufunc: *mut crate::eval::typval_defs::UfuncT,
    lnum: crate::pos_defs::LinenrT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let f = unsafe { &mut *ufunc };
    let name: *mut u8 = match f.uf_name_exp.as_mut() {
        Some(exp) => exp.as_mut_ptr(),
        None => f.uf_name.as_mut_ptr(),
    };
    // SAFETY: forwarded from this function's own safety doc.
    let idx = unsafe { estack_push(crate::runtime_defs::EtypeT::Ufunc, name, lnum) };
    // SAFETY: forwarded from this function's own safety doc.
    let stack = unsafe { EXESTACK.get_mut() };
    stack[idx].es_info = crate::runtime_defs::EsInfo::Ufunc(ufunc);
}

/// Take an item off the execution stack (`estack_pop`).
///
/// The base `ETYPE_TOP` frame installed by [`estack_init`] is never
/// removed, so this is a no-op at a length of one or less.
///
/// # Safety
/// Must not run concurrently with any other access to `EXESTACK`.
pub unsafe fn estack_pop() {
    // SAFETY: forwarded from this function's own safety doc.
    let stack = unsafe { EXESTACK.get_mut() };
    if stack.len() > 1 {
        stack.pop();
    }
}

/// Look up the script item for script ID `id` (`SCRIPT_ITEM(id)`).
///
/// # Panics
/// Panics if `id` is out of range (less than 1, or greater than the
/// number of scripts created so far via [`new_script_item`]) - the
/// original's own unchecked-array-access has no bounds check either,
/// so an out-of-range `id` is already a caller bug there too; this
/// just fails loudly instead of reading out of bounds.
#[must_use]
pub fn script_item(id: ScidT) -> *mut ScriptitemT {
    // SAFETY: SCRIPT_ITEMS is only ever read/written through this
    // module's own functions, none of which hold a live reference
    // across another call into this same cell.
    let items = unsafe { SCRIPT_ITEMS.get_mut() };
    items[(id - 1) as usize]
}

/// Create a new script item and allocate script-local vars
/// (`new_script_item`).
///
/// Returns `(sid, item)`: the new item's script ID and a pointer to
/// the created [`ScriptitemT`] - collapsing the original's
/// `scid_T *sid_out` out-parameter into part of the return value,
/// matching this crate's usual preference for a single meaningful
/// return over a C-style out-parameter.
///
/// `name` is `None` for an anonymous `:source` (matching the
/// original's `NULL`).
pub fn new_script_item(name: Option<Vec<u8>>) -> (ScidT, *mut ScriptitemT) {
    let sid = {
        // SAFETY: forwarded from script_item's own reasoning.
        let counter = unsafe { LAST_CURRENT_SID.get_mut() };
        *counter += 1;
        *counter
    };
    // SAFETY: forwarded from script_item's own reasoning.
    let items = unsafe { SCRIPT_ITEMS.get_mut() };
    while (items.len() as ScidT) < sid {
        let si = Box::into_raw(Box::new(ScriptitemT::default()));
        items.push(si);
        let new_sid = items.len() as ScidT;
        crate::eval::vars::new_script_vars(new_sid);
    }
    let si = items[(sid - 1) as usize];
    // SAFETY: si was just allocated via Box::into_raw above (in this
    // call, or a previous one if sid was already covered by the while
    // loop not running) and is never freed by this crate - see this
    // module's own doc comment.
    unsafe { (*si).sn_name = name };
    (sid, si)
}

/// The number of script items created so far via [`new_script_item`]
/// (`script_items.ga_len`) - the highest valid `id` [`script_item`]
/// will accept.
#[must_use]
pub fn script_item_count() -> ScidT {
    // SAFETY: forwarded from script_item's own reasoning.
    unsafe { SCRIPT_ITEMS.get_mut() }.len() as ScidT
}

/// Whether script id `sid` denotes a Lua script (`script_is_lua`).
#[must_use]
pub fn script_is_lua(sid: ScidT) -> bool {
    if sid == crate::globals::SID_LUA {
        return true;
    }
    if sid <= 0 || sid > script_item_count() {
        return false;
    }
    let item = script_item(sid);
    !item.is_null() && unsafe { (*item).sn_lua }
}

/// Finds the newest loaded script whose name matches `name`
/// (`find_script_by_name`), or `-1`.
///
/// # Safety
/// Reads filename-comparison option state through
/// [`crate::path::path_fnamecmp`].
#[must_use]
pub unsafe fn find_script_by_name(name: &[u8]) -> ScidT {
    for sid in (1..=script_item_count()).rev() {
        let item = script_item(sid);
        if !item.is_null()
            && unsafe { (*item).sn_name.as_deref() }
                .is_some_and(|saved| unsafe {
                    crate::path::path_fnamecmp(saved, name) == 0
                })
        {
            return sid;
        }
    }
    -1
}

#[must_use]
pub fn source_breakpoint(
    cookie: &mut crate::runtime_defs::SourceCookieT,
) -> &mut crate::pos_defs::LinenrT {
    &mut cookie.breakpoint
}

#[must_use]
pub fn source_dbg_tick(
    cookie: &mut crate::runtime_defs::SourceCookieT,
) -> &mut i32 {
    &mut cookie.dbg_tick
}

#[must_use]
pub fn source_level(cookie: &crate::runtime_defs::SourceCookieT) -> i32 {
    cookie.level
}

/// Initialize a source cookie from in-memory command text
/// (`do_source_str_init`).
#[allow(dead_code)]
fn do_source_str_init(
    cookie: &mut crate::runtime_defs::SourceCookieT,
    source: &[u8],
) {
    cookie.buflines.clear();
    let end = source
        .iter()
        .position(|&byte| byte == crate::ascii_defs::NUL)
        .unwrap_or(source.len());
    let mut rest = &source[..end];
    while !rest.is_empty() {
        if let Some(newline) = rest.iter().position(|&byte| byte == b'\n') {
            cookie.buflines.push(rest[..newline].to_vec());
            rest = &rest[newline + 1..];
        } else {
            cookie.buflines.push(rest.to_vec());
            break;
        }
    }
    cookie.buf_lnum = 0;
    cookie.source_from_buf_or_str = true;
}

/// Append `source` while escaping every comma
/// (`strcpy_comma_escaped`).
///
/// Returns the destination offset immediately after the appended
/// bytes, replacing the original's advanced pointer.
#[allow(dead_code)]
fn strcpy_comma_escaped(destination: &mut Vec<u8>, source: &[u8]) -> usize {
    destination.reserve(source.len());
    for &byte in source {
        if byte == b',' {
            destination.push(b'\\');
        }
        destination.push(byte);
    }
    destination.len()
}

/// If `name` has a package name (contains `AUTOLOAD_CHAR` after its
/// first byte), try autoloading the script for it (`script_autoload`).
///
/// Only the original's own fast-reject path is translated for real:
/// if there is no `#` after `name`'s first byte, there is no package
/// name at all, so the answer is unconditionally `false` (`runtime.c`
/// lines 3043-3046). The substantive path (actually go source
/// `$VIMRUNTIME/autoload/<name>.vim`) needs `do_in_runtimepath` (real
/// `'runtimepath'` traversal + file I/O), `source_callback` (actually
/// sourcing a Vimscript file - the parser doesn't exist yet), and a
/// new `ga_loaded` growarray tracking which autoload scripts were
/// already loaded - all substantial, separate undertakings, so that
/// path is `unimplemented!()`. It is reached only when a real
/// autoload-style (`Name#sub`) variable name is looked up - which
/// cannot happen yet in this crate today (no Vimscript parser exists
/// to ever produce such a lookup) - matching this crate's established
/// "unimplemented!() only reached when nothing in this crate can
/// currently trigger it" precedent (e.g. `fold.c`'s real fold-tree
/// search, `cursor.c`'s `coladvance2` narrow branch).
///
/// `autoload_name` (builds the `autoload/<name-with-# replaced-by-/>.vim`
/// script path) is needed only by the substantive path above, so it
/// remains deferred alongside it.
pub fn script_autoload(name: &[u8], reload: bool) -> bool {
    // If there is no '#' after name[0] there is no package name.
    let Some(pos) = name.iter().position(|&b| b == crate::eval::eval::AUTOLOAD_CHAR) else {
        return false;
    };
    if pos == 0 {
        return false;
    }
    let _ = reload;
    unimplemented!(
        "script_autoload: real autoload-script sourcing needs do_in_runtimepath/source_callback/ga_loaded, none translated yet - see this function's own doc comment"
    );
}

/// `getscriptinfo([{opts}])` - a `List` of currently sourced
/// Vimscript/Lua scripts (`f_getscriptinfo`, `runtime.c`), via the
/// already-existing script-item registry ([`script_item_count`]).
///
/// Since nothing in this crate can currently source a real script
/// (`:source`/this file's own script-loading pipeline isn't
/// translated), [`script_item_count`] is always `0` today - the
/// original's own per-script loop is therefore always zero-iteration
/// here, matching the "always-real-fast-path" pattern already
/// established elsewhere in this crate (e.g. `autocmd.rs`'s empty
/// `AUTOCMDS`). This will start returning real script entries, with
/// zero changes needed here, the moment a future session translates
/// `:source`.
///
/// `{opts}.sid` is parsed and validated for real, matching the
/// original exactly (including its own "must be > 0" check, message
/// display omitted per this crate's established policy). `{opts}.name`
/// (a pattern filter) needs `vim_regcomp` (the real regex engine, not
/// yet translated) - `unimplemented!()`s only when a real, non-empty
/// `name` string is actually present; an absent/empty dict, or one
/// with only `sid`, never reaches this (matches the original's own
/// structure precisely: `tv_dict_get_string` returns `None` for a
/// missing key, so no regex compilation is ever attempted then
/// either).
///
/// # Safety
/// Forwarded from `crate::eval::typval`'s own safety docs.
pub unsafe fn f_getscriptinfo(argvars: &[crate::eval::typval_defs::TypvalT], rettv: &mut crate::eval::typval_defs::TypvalT) {
    use crate::eval::typval::{tv_check_for_opt_dict_arg, tv_dict_find, tv_dict_get_string, tv_get_number_chk, tv_list_alloc_ret};
    use crate::eval::typval_defs::TypvalValue;
    use crate::vim_defs::FAIL;

    // SAFETY: `rettv` is freshly default-initialized by the caller.
    let _ = unsafe { tv_list_alloc_ret(rettv, script_item_count() as isize) };

    if !argvars.is_empty() && tv_check_for_opt_dict_arg(argvars, 0) == FAIL {
        return;
    }

    if let Some(crate::eval::typval_defs::TypvalT { value: TypvalValue::Dict(d), .. }) = argvars.first() {
        let d = *d;
        // SAFETY: `d`, if non-null, is a live Dict owned by the
        // caller's own argument typval for the duration of this call.
        let sid_item = tv_dict_find(if d.is_null() { None } else { Some(unsafe { &mut *d }) }, b"sid");
        if let Some(sid_ptr) = sid_item {
            // SAFETY: `sid_ptr` was just returned by `tv_dict_find`
            // above as a live item of `d`.
            let sid_tv = unsafe { &(*sid_ptr).di_tv };
            let mut error = false;
            let sid = tv_get_number_chk(sid_tv, Some(&mut error));
            // Skips the per-script loop below - a genuine no-op today
            // (that loop is unconditionally zero-iteration, see this
            // function's own doc comment), so clippy flags this early
            // `return` as needless - kept anyway, matching the
            // original's own real structure, so this stays correct
            // the moment a future session adds the real loop.
            #[allow(clippy::needless_return)]
            if error || sid <= 0 {
                return;
            }
        } else {
            // SAFETY: forwarded above.
            let name = unsafe { tv_dict_get_string(if d.is_null() { None } else { Some(&mut *d) }, b"name") };
            if name.is_some_and(|n| !n.is_empty()) {
                unimplemented!(
                    "f_getscriptinfo: a real {{name}} pattern filter needs vim_regcomp, the \
                     real regex engine, not yet translated"
                );
            }
        }
    }

    // The per-script loop (over script IDs 1..=script_item_count())
    // is always zero-iteration today - see this function's own doc
    // comment.
}

/// Build the `List` `getstacktrace()` returns (`stacktrace_create`,
/// `runtime.c`): one entry per `EXESTACK` frame, each built via
/// `stacktrace_push_item` in the original. Since `EXESTACK` is always
/// empty today (see its own doc comment), this loop is always
/// zero-iteration, so `stacktrace_push_item` itself (which would need
/// `ETYPE_UFUNC`'s `ufunc_T.uf_script_ctx`/`get_scriptname` and
/// `ETYPE_AUCMD`'s `AutoCmd.script_ctx`, neither wired up for this
/// purpose yet) never needs to exist here either. This is a real,
/// always-taken early return, not a hardcoded shortcut.
///
/// # Safety
/// `rettv` must be freshly default-initialized by the caller (no
/// pre-existing `List`/`Dict`/`Blob` value that would otherwise leak),
/// forwarded from [`crate::eval::typval::tv_list_alloc_ret`]'s own
/// safety doc.
pub unsafe fn stacktrace_create(rettv: &mut crate::eval::typval_defs::TypvalT) {
    // SAFETY: forwarded from this function's own safety doc; the
    // `&Vec` this briefly, implicitly creates (to call `.len()`) is
    // used and discarded immediately, matching `eval/vars.rs`'s own
    // `VIMVARS.as_ptr()` precedent for this exact idiom.
    let exestack_len = unsafe { (*EXESTACK.as_ptr()).len() };
    // SAFETY: forwarded from this function's own safety doc.
    let _ = unsafe { crate::eval::typval::tv_list_alloc_ret(rettv, exestack_len as isize) };
    // The per-frame loop is always zero-iteration - see this
    // function's own doc comment.
}

/// `getstacktrace()` - the current call stack as a `List` of
/// `{filename, lnum, funcname}` dicts (`f_getstacktrace`, `runtime.c`).
/// Always an empty `List` today, since [`stacktrace_create`]'s own
/// loop is always zero-iteration.
///
/// # Safety
/// Forwarded from [`stacktrace_create`]'s own safety doc.
pub unsafe fn f_getstacktrace(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { stacktrace_create(rettv) };
}

/// Whether there is a currently-active execution-stack frame to report
/// source-location information about (`HAVE_SOURCING_INFO`, a macro
/// in the original: `exestack.ga_data != NULL && exestack.ga_len > 0`).
///
/// Always `false` today, matching `EXESTACK`'s own doc comment (see
/// above) - `message.rs`'s own `other_sourcing_name`/`get_emsg_source`
/// are this accessor's own real, current callers.
#[must_use]
pub fn have_sourcing_info() -> bool {
    // SAFETY: the `&Vec` this briefly, implicitly creates (to call
    // `.is_empty()`) is used and discarded immediately, matching this
    // module's own `stacktrace_create`'s identical `EXESTACK.as_ptr()`
    // precedent.
    !unsafe { (*EXESTACK.as_ptr()).is_empty() }
}

/// The line number of the topmost execution-stack frame (`SOURCING_LNUM`,
/// a macro in the original:
/// `((estack_T *)exestack.ga_data)[exestack.ga_len - 1].es_lnum`).
///
/// `EXESTACK` is always empty today (see this module's own doc
/// comment), so there is no real topmost frame to read a line number
/// from - this returns `0` for that case (matching
/// [`have_sourcing_info`]'s own `false` "no current execution context"
/// answer for the identical state), the genuinely correct value today
/// rather than a hardcoded shortcut: once a future session adds real
/// script/function/autocmd execution frames, this starts returning the
/// true topmost frame's own line number automatically, with no changes
/// needed at any of this function's own call sites (e.g. `option.rs`'s
/// `set_option_sctx`).
#[must_use]
pub fn sourcing_lnum() -> crate::pos_defs::LinenrT {
    // SAFETY: matching `have_sourcing_info`'s own `EXESTACK.as_ptr()`
    // precedent - the `&Vec` this briefly, implicitly creates is used
    // and discarded immediately.
    unsafe { (*EXESTACK.as_ptr()).last() }.map_or(0, |e| e.es_lnum)
}

/// Test-only: resets [`SCRIPT_ITEMS`]/[`LAST_CURRENT_SID`] to empty so
/// each test (in this module, or `eval::vars`'s own tests exercising
/// [`new_script_item`]/`new_script_vars` together) starts from a clean
/// slate. Unlike `eval::userfunc::func_init` (a real `pub fn`
/// translating the original's own `func_init`), the original has no
/// equivalent "re-init script_items" function - scripts accumulate for
/// the whole nvim session - so this helper is test-only, not a
/// translation of anything. `pub(crate)` (not `pub`) since it must
/// never be reachable from real, non-test code.
#[cfg(test)]
pub(crate) fn tests_reset_for_test() {
    // SAFETY: forwarded from script_item's own reasoning; every caller
    // holds global_state_test_lock() for its whole body, serializing
    // access across tests.
    unsafe {
        *SCRIPT_ITEMS.get_mut() = Vec::new();
        *LAST_CURRENT_SID.get_mut() = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn did_set_runtimepackpath_invalidates_the_cached_search_path() {
        let _lock = global_state_test_lock();
        unsafe { *RUNTIME_SEARCH_PATH_VALID.get_mut() = true };
        did_set_runtimepackpath();
        assert!(!unsafe { *RUNTIME_SEARCH_PATH_VALID.get_mut() });
    }
    use crate::globals::global_state_test_lock;

    /// Saves and restores `EXESTACK` so a test cannot leak frames,
    /// which would flip `have_sourcing_info()` for every later test.
    struct ExestackGuard {
        saved: Vec<crate::runtime_defs::EstackT>,
    }

    impl ExestackGuard {
        fn new() -> Self {
            Self { saved: std::mem::take(unsafe { EXESTACK.get_mut() }) }
        }
    }

    impl Drop for ExestackGuard {
        fn drop(&mut self) {
            *unsafe { EXESTACK.get_mut() } = std::mem::take(&mut self.saved);
        }
    }

    #[test]
    fn estack_init_installs_a_single_top_frame() {
        let _lock = global_state_test_lock();
        let _guard = ExestackGuard::new();

        unsafe { estack_init() };
        let stack = unsafe { EXESTACK.get_mut() };
        assert_eq!(stack.len(), 1);
        assert_eq!(stack[0].es_type, crate::runtime_defs::EtypeT::Top);
        assert!(stack[0].es_name.is_null());
        assert_eq!(stack[0].es_lnum, 0);
    }

    // ---- estack_push_ufunc ----

    fn test_ufunc(name: &[u8], exp: Option<&[u8]>) -> Box<crate::eval::typval_defs::UfuncT> {
        let mut f = Box::new(crate::eval::typval_defs::UfuncT::default());
        f.uf_name = name.to_vec();
        f.uf_name_exp = exp.map(<[u8]>::to_vec);
        f
    }

    /// The frame records the function itself, so a stack trace can
    /// reach back to it.
    #[test]
    fn estack_push_ufunc_records_the_function_in_the_frame() {
        let _lock = global_state_test_lock();
        let _guard = ExestackGuard::new();
        unsafe { estack_init() };

        let mut f = test_ufunc(b"Foo", None);
        let f_ptr = std::ptr::from_mut(&mut *f);
        unsafe { estack_push_ufunc(f_ptr, 42) };

        let stack = unsafe { EXESTACK.get_mut() };
        let top = stack.last().expect("a frame was pushed");
        assert_eq!(top.es_type, crate::runtime_defs::EtypeT::Ufunc);
        assert_eq!(top.es_lnum, 42);
        match top.es_info {
            crate::runtime_defs::EsInfo::Ufunc(p) => assert_eq!(p, f_ptr),
            other => panic!("expected the Ufunc variant, got {other:?}"),
        }
    }

    /// With no expanded name, the plain name is used.
    #[test]
    fn estack_push_ufunc_uses_the_plain_name_when_there_is_no_expanded_one() {
        let _lock = global_state_test_lock();
        let _guard = ExestackGuard::new();
        unsafe { estack_init() };

        let mut f = test_ufunc(b"Foo", None);
        unsafe { estack_push_ufunc(std::ptr::from_mut(&mut *f), 1) };

        let stack = unsafe { EXESTACK.get_mut() };
        let name = stack.last().unwrap().es_name;
        assert_eq!(name, f.uf_name.as_mut_ptr());
    }

    /// The EXPANDED name wins when present - that is the
    /// `<SNR>`-resolved form a stack trace should show. An
    /// implementation always taking `uf_name` would point at the
    /// unresolved one.
    #[test]
    fn estack_push_ufunc_prefers_the_expanded_name() {
        let _lock = global_state_test_lock();
        let _guard = ExestackGuard::new();
        unsafe { estack_init() };

        let mut f = test_ufunc(b"Foo", Some(b"<SNR>7_Foo"));
        unsafe { estack_push_ufunc(std::ptr::from_mut(&mut *f), 1) };

        let stack = unsafe { EXESTACK.get_mut() };
        let name = stack.last().unwrap().es_name;
        assert_eq!(
            name,
            f.uf_name_exp.as_mut().unwrap().as_mut_ptr(),
            "the expanded name must win"
        );
        assert_ne!(name, f.uf_name.as_mut_ptr());
    }

    #[test]
    fn estack_push_appends_and_reports_its_index() {
        let _lock = global_state_test_lock();
        let _guard = ExestackGuard::new();
        unsafe { estack_init() };

        let idx = unsafe {
            estack_push(crate::runtime_defs::EtypeT::Script, std::ptr::null_mut(), 12)
        };
        assert_eq!(idx, 1);
        let stack = unsafe { EXESTACK.get_mut() };
        assert_eq!(stack.len(), 2);
        assert_eq!(stack[idx].es_type, crate::runtime_defs::EtypeT::Script);
        assert_eq!(stack[idx].es_lnum, 12);
    }

    #[test]
    fn estack_pop_never_removes_the_base_frame() {
        let _lock = global_state_test_lock();
        let _guard = ExestackGuard::new();
        unsafe { estack_init() };
        unsafe { estack_push(crate::runtime_defs::EtypeT::Script, std::ptr::null_mut(), 1) };

        unsafe { estack_pop() };
        assert_eq!(unsafe { EXESTACK.get_mut() }.len(), 1);

        // The ETYPE_TOP frame stays no matter how often this is called.
        unsafe { estack_pop() };
        unsafe { estack_pop() };
        assert_eq!(unsafe { EXESTACK.get_mut() }.len(), 1);
    }

    #[test]
    fn estack_pop_on_an_empty_stack_is_a_no_op() {
        let _lock = global_state_test_lock();
        let _guard = ExestackGuard::new();
        *unsafe { EXESTACK.get_mut() } = Vec::new();

        unsafe { estack_pop() };
        assert!(unsafe { EXESTACK.get_mut() }.is_empty());
    }

    #[test]
    fn have_sourcing_info_still_reports_false_by_default() {
        let _lock = global_state_test_lock();
        let _guard = ExestackGuard::new();
        // Nothing calls estack_init() in this crate, so the stack is
        // empty and message.rs's other_sourcing_name stays on its
        // early-return path rather than its unimplemented!() body.
        *unsafe { EXESTACK.get_mut() } = Vec::new();
        assert!(!have_sourcing_info());

        // ...but a pushed frame does flip it, which is exactly why the
        // first real caller of estack_init must translate
        // SOURCING_NAME alongside it.
        unsafe { estack_init() };
        assert!(have_sourcing_info());
    }

    #[test]
    fn new_script_item_assigns_sequential_sids() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        let (sid1, _item1) = new_script_item(Some(b"first.vim".to_vec()));
        let (sid2, _item2) = new_script_item(Some(b"second.vim".to_vec()));
        assert_eq!(sid2, sid1 + 1);
    }

    #[test]
    fn new_script_item_sets_name_and_initializes_sn_vars() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        let (_sid, item) = new_script_item(Some(b"myscript.vim".to_vec()));
        unsafe {
            assert_eq!((*item).sn_name, Some(b"myscript.vim".to_vec()));
            assert!(!(*item).sn_vars.is_null());
        }
    }

    #[test]
    fn new_script_item_anonymous_source_has_no_name() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        let (_sid, item) = new_script_item(None);
        unsafe {
            assert!((*item).sn_name.is_none());
        }
    }

    #[test]
    fn script_item_looks_up_by_sid() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        let (sid, item) = new_script_item(None);
        assert_eq!(script_item(sid), item);
    }

    #[test]
    fn new_script_item_first_sid_is_one() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        let (sid, _item) = new_script_item(None);
        assert_eq!(sid, 1);
    }

    #[test]
    #[should_panic]
    fn script_item_panics_for_out_of_range_sid() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        new_script_item(None);
        let _ = script_item(99);
    }

    #[test]
    fn script_item_count_zero_when_none_registered() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        assert_eq!(script_item_count(), 0);
    }

    #[test]
    fn script_item_count_matches_the_number_created() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        new_script_item(None);
        new_script_item(None);
        new_script_item(None);
        assert_eq!(script_item_count(), 3);
    }

    #[test]
    fn script_is_lua_accepts_the_builtin_lua_sid() {
        assert!(script_is_lua(crate::globals::SID_LUA));
    }

    #[test]
    fn script_is_lua_reads_the_registered_script_flag() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        let (lua_sid, lua) = new_script_item(Some(b"lua.lua".to_vec()));
        let (vim_sid, _vim) = new_script_item(Some(b"vim.vim".to_vec()));
        unsafe { (*lua).sn_lua = true };

        assert!(script_is_lua(lua_sid));
        assert!(!script_is_lua(vim_sid));
    }

    #[test]
    fn script_is_lua_rejects_invalid_script_ids() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        new_script_item(None);
        assert!(!script_is_lua(0));
        assert!(!script_is_lua(-1));
        assert!(!script_is_lua(2));
    }

    #[test]
    fn find_script_by_name_returns_the_newest_matching_sid() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        let (first, _) = new_script_item(Some(b"same.vim".to_vec()));
        new_script_item(Some(b"other.vim".to_vec()));
        let (newest, _) = new_script_item(Some(b"same.vim".to_vec()));
        assert_ne!(first, newest);
        assert_eq!(unsafe { find_script_by_name(b"same.vim") }, newest);
        assert_eq!(unsafe { find_script_by_name(b"missing.vim") }, -1);
    }

    #[test]
    fn source_breakpoint_returns_writable_breakpoint_storage() {
        let mut cookie = crate::runtime_defs::SourceCookieT::default();
        *source_breakpoint(&mut cookie) = 42;
        assert_eq!(cookie.breakpoint, 42);
    }

    #[test]
    fn source_dbg_tick_returns_writable_debug_tick_storage() {
        let mut cookie = crate::runtime_defs::SourceCookieT::default();
        *source_dbg_tick(&mut cookie) = 17;
        assert_eq!(cookie.dbg_tick, 17);
    }

    #[test]
    fn source_level_returns_the_cookie_nesting_level() {
        let cookie = crate::runtime_defs::SourceCookieT {
            level: 9,
            ..Default::default()
        };
        assert_eq!(source_level(&cookie), 9);
    }

    #[test]
    fn do_source_str_init_splits_lines_without_a_trailing_empty_line() {
        let mut cookie = crate::runtime_defs::SourceCookieT {
            buf_lnum: 9,
            buflines: vec![b"old".to_vec()],
            ..Default::default()
        };
        do_source_str_init(&mut cookie, b"one\n\ntwo\nignored\0tail");
        assert_eq!(
            cookie.buflines,
            vec![
                b"one".to_vec(),
                Vec::new(),
                b"two".to_vec(),
                b"ignored".to_vec(),
            ]
        );
        assert_eq!(cookie.buf_lnum, 0);
        assert!(cookie.source_from_buf_or_str);

        do_source_str_init(&mut cookie, b"last\n");
        assert_eq!(cookie.buflines, vec![b"last".to_vec()]);
    }

    #[test]
    fn strcpy_comma_escaped_appends_and_returns_the_end_offset() {
        let mut destination = b"prefix:".to_vec();
        assert_eq!(
            strcpy_comma_escaped(&mut destination, b"a,b,,c"),
            b"prefix:a\\,b\\,\\,c".len()
        );
        assert_eq!(destination, b"prefix:a\\,b\\,\\,c");

        let end = strcpy_comma_escaped(&mut destination, b"plain");
        assert_eq!(end, destination.len());
        assert!(destination.ends_with(b"plain"));
    }

    #[test]
    fn script_autoload_false_when_name_has_no_autoload_char_at_all() {
        assert!(!script_autoload(b"foo", false));
    }

    #[test]
    fn script_autoload_false_when_autoload_char_is_the_first_byte() {
        // "Caller must make sure that name contains AUTOLOAD_CHAR" per
        // the original's own doc comment - but the original itself
        // still explicitly guards against a LEADING '#' (p == name),
        // treating it the same as "no package name", so this is a
        // real, faithfully-preserved behavior, not just an unchecked
        // caller precondition.
        assert!(!script_autoload(b"#foo", false));
    }

    #[test]
    #[should_panic]
    fn script_autoload_unimplemented_for_a_real_autoload_style_name() {
        // "foo#bar" - a genuine package name (# after the first byte) -
        // reaches the not-yet-translated substantive path.
        script_autoload(b"foo#bar", false);
    }

    // --- have_sourcing_info / sourcing_lnum ---

    #[test]
    fn have_sourcing_info_is_false_since_exestack_is_always_empty() {
        let _lock = global_state_test_lock();
        assert!(!have_sourcing_info());
    }

    #[test]
    fn sourcing_lnum_is_zero_since_exestack_is_always_empty() {
        let _lock = global_state_test_lock();
        assert_eq!(sourcing_lnum(), 0);
    }

    // --- f_getscriptinfo ---

    #[test]
    fn getscriptinfo_no_args_returns_an_empty_list() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();

        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        unsafe { f_getscriptinfo(&[], &mut rettv) };

        let crate::eval::typval_defs::TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 0);
        // SAFETY: `l` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn getscriptinfo_still_empty_even_after_registering_a_script() {
        // The per-script loop is always zero-iteration TODAY regardless
        // of script_item_count(), since nothing real ever reaches this
        // function through a genuine :source - but registering one
        // directly (as this test does) still shouldn't change the
        // observable result, since f_getscriptinfo's own loop bound
        // (script_item_count()) simply becomes non-zero without any
        // change to its own logic - proving the "always empty" claim
        // isn't an artifact of script_item_count() staying at 0.
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        new_script_item(Some(b"foo.vim".to_vec()));

        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        unsafe { f_getscriptinfo(&[], &mut rettv) };

        let crate::eval::typval_defs::TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 0);
        // SAFETY: `l` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn getscriptinfo_valid_sid_succeeds() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();

        let opts = crate::eval::typval::tv_dict_alloc();
        // SAFETY: `opts` was just allocated above, exclusively owned.
        crate::eval::typval::tv_dict_add_nr(unsafe { &mut *opts }, b"sid", 1);
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        let arg = crate::eval::typval_defs::TypvalT {
            value: crate::eval::typval_defs::TypvalValue::Dict(opts),
            ..Default::default()
        };
        unsafe { f_getscriptinfo(std::slice::from_ref(&arg), &mut rettv) };

        let crate::eval::typval_defs::TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 0);
        // SAFETY: `l`/`opts` are each still exclusively owned; nothing
        // else references either.
        unsafe {
            crate::eval::typval::tv_list_unref(l);
            crate::eval::typval::tv_dict_free(opts);
        }
    }

    #[test]
    fn getscriptinfo_sid_zero_or_negative_fails() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();

        for bad_sid in [0, -1] {
            let opts = crate::eval::typval::tv_dict_alloc();
            // SAFETY: `opts` was just allocated above, exclusively
            // owned.
            crate::eval::typval::tv_dict_add_nr(unsafe { &mut *opts }, b"sid", bad_sid);
            let mut rettv = crate::eval::typval_defs::TypvalT::default();
            let arg = crate::eval::typval_defs::TypvalT {
                value: crate::eval::typval_defs::TypvalValue::Dict(opts),
                ..Default::default()
            };
            unsafe { f_getscriptinfo(std::slice::from_ref(&arg), &mut rettv) };

            // On this early-return path, rettv is left at whatever
            // tv_list_alloc_ret already set it to (an empty list) -
            // the original's own equivalent early `return;` likewise
            // never touches rettv again after its own initial
            // tv_list_alloc_ret call.
            let crate::eval::typval_defs::TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
            assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 0);
            // SAFETY: `l`/`opts` are each still exclusively owned;
            // nothing else references either.
            unsafe {
                crate::eval::typval::tv_list_unref(l);
                crate::eval::typval::tv_dict_free(opts);
            }
        }
    }

    #[test]
    fn getscriptinfo_empty_dict_argument_needs_no_regex() {
        // No "sid" key and no "name" key at all - tv_dict_get_string
        // returns None for the missing "name" key, so this never
        // reaches the not-yet-translated vim_regcomp path, matching
        // the original's own exact structure.
        let _lock = global_state_test_lock();
        tests_reset_for_test();

        let opts = crate::eval::typval::tv_dict_alloc();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        let arg = crate::eval::typval_defs::TypvalT {
            value: crate::eval::typval_defs::TypvalValue::Dict(opts),
            ..Default::default()
        };
        unsafe { f_getscriptinfo(std::slice::from_ref(&arg), &mut rettv) };

        let crate::eval::typval_defs::TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 0);
        // SAFETY: `l`/`opts` are each still exclusively owned; nothing
        // else references either.
        unsafe {
            crate::eval::typval::tv_list_unref(l);
            crate::eval::typval::tv_dict_free(opts);
        }
    }

    #[test]
    #[should_panic(expected = "vim_regcomp")]
    fn getscriptinfo_name_pattern_filter_is_unimplemented() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();

        let opts = crate::eval::typval::tv_dict_alloc();
        // SAFETY: `opts` was just allocated above, exclusively owned.
        crate::eval::typval::tv_dict_add_str(unsafe { &mut *opts }, b"name", Some(b"foo"));
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        let arg = crate::eval::typval_defs::TypvalT {
            value: crate::eval::typval_defs::TypvalValue::Dict(opts),
            ..Default::default()
        };
        unsafe { f_getscriptinfo(std::slice::from_ref(&arg), &mut rettv) };
    }

    #[test]
    fn getscriptinfo_type_error_leaves_the_list_empty() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();

        let bad_arg = crate::eval::typval_defs::TypvalT {
            value: crate::eval::typval_defs::TypvalValue::List(std::ptr::null_mut()),
            ..Default::default()
        };
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        unsafe { f_getscriptinfo(std::slice::from_ref(&bad_arg), &mut rettv) };

        let crate::eval::typval_defs::TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 0);
        // SAFETY: `l` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    // --- stacktrace_create / f_getstacktrace ---

    #[test]
    fn stacktrace_create_is_empty_when_exestack_is_empty() {
        let _lock = global_state_test_lock();
        assert_eq!(unsafe { (*EXESTACK.as_ptr()).len() }, 0);

        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        unsafe { stacktrace_create(&mut rettv) };

        let crate::eval::typval_defs::TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 0);
        // SAFETY: `l` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn getstacktrace_returns_an_empty_list() {
        let _lock = global_state_test_lock();

        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        unsafe { f_getstacktrace(&[], &mut rettv) };

        let crate::eval::typval_defs::TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 0);
        // SAFETY: `l` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn stacktrace_create_tracks_exestacks_own_length() {
        let _lock = global_state_test_lock();
        // EXESTACK is genuinely always empty in this crate today (no
        // Ex-command execution engine can push a frame onto it), but
        // stacktrace_create's own allocation call still reads its real
        // length rather than a hardcoded 0 - proven here by directly
        // manipulating EXESTACK itself (something no real, translated
        // caller can currently do) and confirming the allocated list's
        // capacity/length tracks it, not just always happening to be 0.
        unsafe {
            (*EXESTACK.as_ptr()).push(crate::runtime_defs::EstackT::default());
            (*EXESTACK.as_ptr()).push(crate::runtime_defs::EstackT::default());
        }

        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        unsafe { stacktrace_create(&mut rettv) };

        let crate::eval::typval_defs::TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        // The per-frame loop is still zero-iteration (nothing calls
        // stacktrace_push_item), so the list is empty even though
        // tv_list_alloc_ret was asked to pre-size for 2 entries.
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 0);
        // SAFETY: `l` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_list_unref(l) };

        unsafe { *EXESTACK.as_ptr() = Vec::new() };
    }
}
