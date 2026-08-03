//! Translated from `src/nvim/eval/vars.c` (tractable core only).
//!
//! `vars.c` (~2700 lines) implements Vimscript variable get/set/unlet,
//! `:let`/`:unlet`/`:const` command execution, and the `g:`/`b:`/`w:`/
//! `t:`/`s:`/`l:`/`a:`/`v:` scope-dictionary machinery - almost all of
//! it needs the expression evaluator and `ex_cmds.lua`-generated
//! command dispatch, not attempted here.
//!
//! Translated: `init_var_dict` - the small, self-contained scope-dict
//! initializer shared by every scope (`s:`, and (per its own doc
//! comment) `b:`/`w:`/`t:` too, wherever those are eventually wired up
//! for real). Needed only already-existing pieces: `HashtabT::hash_init`,
//! `VarLockStatus`, `ScopeType`, `DO_NOT_FREE_CNT`, `dict_item_flags`.
//!
//! Also translated: `new_script_vars` - tractable now that
//! `crate::runtime`'s `script_items`/`new_script_item` exist. Builds a
//! fresh, zeroed `ScriptvarT` (matching the original's own
//! `xcalloc(1, sizeof(scriptvar_T))`, NOT
//! [`crate::eval::typval::tv_dict_alloc`], since a script-scope dict
//! has `dv_refcount == DO_NOT_FREE_CNT` and is deliberately never
//! linked into the `GC_FIRST_DICT` used-dicts list, matching the
//! original exactly), calls [`init_var_dict`] on it, then wires the
//! result into the script item at `id` via
//! `crate::runtime::script_item`.
//!
//! The original's `QUEUE_INIT(&dict->watchers)` is omitted - `DictT`
//! has no `watchers` field at all yet (needs a `QUEUE` intrusive-
//! linked-list translation first, same accepted gap as documented on
//! `DictT` itself in `eval/typval_defs.rs`).
//!
//! Also translated: the `v:` special-variable storage layer -
//! `eval_defs.h`'s `VimVarIndex` enum (embedded here directly, since
//! `eval_defs.h` has no dedicated `_defs.rs` module of its own - same
//! treatment as `charset.h`'s `vim_isbreak` in `charset.rs`) plus
//! `vars.c`'s own `vimvars[]` table (as `VIMVARS`) and its accessors:
//! `get_vim_var_tv`/`get_vim_var_name`/`get_vim_var_nr`/
//! `get_vim_var_list`/`get_vim_var_dict`/`get_vim_var_str`/
//! `get_vim_var_partial`/`set_vim_var_tv`/`set_vim_var_type`/
//! `set_vim_var_nr`/`set_vim_var_bool`/`set_vim_var_special`/
//! `set_vim_var_string`/`set_vim_var_list`/`set_vim_var_dict`/
//! `set_vim_var_partial`/`set_vim_var_char`/`set_reg_var`. This was
//! investigated specifically to unblock `search.c`'s
//! `set_vv_searchforward` (a repeatedly-cited blocker this session) -
//! turned out to be a real, self-contained subsystem once actually
//! examined, much like `plines.c`'s "always-real-fast-path" unlock.
//!
//! `VIMVARS`' 108 entries are mechanically transcribed from the
//! original's own static initializer (`VV(idx, name, type, flags)`
//! macro expansions) - **indexed by [`VimVarIndex`]'s enum order, NOT
//! the table's own textual order**: the original uses C99 designated
//! initializers (`[idx] = {...}`), and at least one pair
//! (`VV_TERMREQUEST`/`VV_TERMRESPONSE`) is declared in a DIFFERENT
//! order in the table than in the enum - verified by cross-referencing
//! every single name between the enum and the table programmatically
//! (both lists contain exactly the same 108 names, zero missing/extra/
//! duplicated) before transcribing, not just assumed from visual
//! inspection. Each entry's own `di.di_tv` matches EXACTLY what the
//! original's macro produces BEFORE `evalvars_init()` ever runs (a
//! zero-valued `vval` of the entry's declared type - `Number(0)`,
//! `String(None)`, a null `List`/`Dict`/`Blob`/`Partial` pointer,
//! `Bool(BoolVarValue::False)`, or `Special(SpecialVarValue::Null)`).
//! `di_flags`/`di_key` ARE populated for real (derived purely from
//! each entry's own `name`/`flags`, mirroring `evalvars_init`'s
//! per-entry loop exactly - see `VIMVARDICT`'s own doc comment).
//!
//! `evalvars_init` itself (which overrides several entries' VALUES with
//! real startup values - `v:count1`/`v:hlsearch`/`v:searchforward` all
//! become `1`, `v:true` becomes `Bool(True)`, `v:errors` gets a real
//! empty list, `v:completed_item`/`v:event` become real, empty,
//! `Fixed`-locked dicts, `v:lua` becomes a real bound `Partial`, etc.)
//! IS now translated - see its own doc comment for exactly which 2
//! pieces remain deliberately deferred (`v:version`/`v:versionlong`,
//! `v:msgpack_types`, and `v:startreason`'s env-var-based override).
//! Wiring every entry into a real `v:` scope `DictT` via `hash_add`
//! (`evalvars_init`'s OTHER job) was already done before this function
//! itself landed - see `VIMVARDICT`.
//!

//! `set_vim_var_type`/`set_vim_var_nr`/`set_vim_var_partial` preserve
//! the original's own peculiar "doesn't set the type" contract (a raw
//! C union write that only makes sense given the caller already knows
//! the slot's real type) as a documented panic-on-mismatch instead: verified
//! every real call site in the original only ever targets an
//! already-correctly-typed slot (e.g. `set_vim_var_type` is ALWAYS
//! immediately followed by `set_vim_var_nr` in every real caller,
//! always passing `VAR_NUMBER`), so this is a faithful "must only be
//! called on a slot of this type" contract, not a narrowing - matching
//! this crate's established `get_op_type` precedent for such
//! caller-contract violations.
//!
//! Also translated (found via a full function-name diff of this file
//! against the real C source, the same methodology used to mine
//! `eval/typval.c`/`eval/userfunc.c` over previous sessions):
//! `set_vcount` (sets `v:count`/`v:count1`/`v:prevcount`, layered
//! directly on the already-real `get_vim_var_nr`/`set_vim_var_nr`) and
//! `valid_varname` (checks every character of a candidate variable
//! name - needed `eval.c`'s own small, self-contained
//! `eval_isnamec`/`eval_isnamec1`, added to `eval/eval.rs` alongside;
//! neither has any `g_chartab`/options-engine dependency, unlike the
//! superficially similar `vim_isIDc`). `valid_varname`'s own
//! `semsg(_(e_illvar), varname)` on the first invalid character is
//! omitted (message display, not tractable yet) - the boolean result
//! itself is kept exactly.
//!
//! Also translated: `var_check_ro`/`var_check_lock`/`var_check_fixed` -
//! the read-only/locked/fixed variable-assignment guards, operating
//! directly on `DictitemT.di_flags` (plus `GLOBALS.sandbox` for
//! `var_check_ro`'s sandbox-specific check). Drop the original's
//! `name`/`name_len` parameters entirely, matching
//! `value_check_lock`/`tv_check_lock`'s own already-established
//! precedent (`eval/typval.rs`) for this exact pattern - those
//! parameters only ever feed the omitted `semsg()` message text, never
//! affecting the returned bool.
//!
//! Also translated: `unref_var_dict` (layered directly on the
//! already-real `tv_dict_unref`) and `vars_clear`/`vars_clear_ext`
//! (frees every item in a scope dict's hashtable, optionally clearing
//! each item's value first). Both take `&mut DictT`/`*mut DictT`
//! rather than the original's bare `hashtab_T*` - every real caller
//! (`buffer.c`'s `b_vars`, `window.c`'s `w_vars`/`t_vars`,
//! `eval/userfunc.c`'s `fc_l_vars`/`fc_l_avars`, this file's own
//! script-vars) only ever passes `&owning_dict.dv_hashtab`, and this
//! crate's `DictT.dv_index` side table (substituting for the
//! original's `TV_DICT_HI2DI` pointer-arithmetic recovery) needs the
//! owning `DictT` itself, not just its bare hashtable, to look items
//! back up - see each function's own doc comment for the full
//! reasoning. `vars_clear_ext`'s core loop mirrors
//! `tv_dict_free_contents`'s own already-established `dv_index`-driven
//! iteration (`eval/typval.rs`), conditionally skipping the
//! `tv_clear_simple` step per `free_val`.
//!
//! Also translated: `garbage_collect_globvars`/`garbage_collect_scriptvars`/
//! `garbage_collect_vimvars`, all now real, thin wrappers around
//! `eval/eval.rs`'s own `set_ref_in_ht`, now that it exists (see that
//! module's own doc comment for the full GC mark-phase family this
//! belongs to). `garbage_collect_scriptvars` needed one small new
//! `crate::runtime::script_item_count` accessor (`script_items.ga_len`
//! in the original) alongside the already-real `script_item`.
//! `garbage_collect_vimvars` needed the new `VIMVARDICT`/
//! `get_vimvar_dict` described below.
//!
//! Also translated: `find_var_ht_dict`/`find_var_ht` - the core
//! scope-prefix (`g:`/`b:`/`w:`/`t:`/`a:`/`l:`/`s:`/`v:`, or implicit)
//! resolution used throughout `:let`/expression evaluation. Reused
//! `BufT.b_vars`/`WinT.w_vars`/`TabpageT.tp_vars` (all three already
//! real fields, just never populated by anything yet - a pleasant
//! surprise found while investigating this function) and
//! `GLOBALS.curbuf`/`curwin`/`curtab`. The `s:` branch's lazy
//! script-item creation (for an anonymous, string-sourced or Lua
//! script context) is translated in full, including the real side
//! effect of updating `GLOBALS.current_sctx.sc_sid` - verified via a
//! dedicated test. The original's `nlua_set_sctx` call inside that
//! branch is omitted (only resolves a Lua filename/line number for
//! "last set" diagnostic messages, confirmed by reading its own body
//! that it never touches `sc_sid` itself, so this omission doesn't
//! change which dict is ultimately resolved).
//!
//! Also translated (this turn): the `v:` scope branch, plus two new
//! file-statics it and `garbage_collect_vimvars` both need:
//! `VIMVARDICT` (`vimvardict`, the real `v:` scope `DictT`) and a
//! now-self-populating `COMPAT_HASHTAB` (previously built but always
//! left empty - see each static's own doc comment for the full
//! reasoning). This required reshaping `Vimvar` itself: `tv: TypvalT`
//! became a real embedded `di: DictitemT` (matching the original's own
//! `TV_DICTITEM_STRUCT(...) vv_di` substruct exactly), so `VIMVARS`'s
//! entries could be addressed directly by `vimvarht`'s hash items -
//! avoiding both design options flagged as risky in an earlier pass
//! (reshaping into something `dv_index`-incompatible, or a
//! synchronized-copy second dict) by making the embedded `DictitemT`
//! itself the single source of truth, addressed two ways (by index via
//! `VIMVARS`, and by name via `VIMVARDICT`'s `dv_hashtab`/`dv_index`),
//! exactly like the original's own `vimvars[i].vv_di`. `evalvars_init`'s
//! OWN remaining body (real per-variable startup defaults - `v:version`,
//! `v:argv`, the msgpack-types dict, etc. - needing
//! `min_vim_version`/`os_getenv`/`tv_dict_alloc_lock`/other not-yet-
//! translated subsystems) is still deliberately deferred, matching
//! `GLOBVARDICT`'s own "bare, pre-`init_var_dict`" precedent for
//! `dv_scope`/`dv_refcount`/a `vimvars_var` static - nothing translated
//! so far reads any of those on a scope dict.
//!
//! `find_var`/`find_var_in_ht`/`find_var_in_scoped_ht` (the next layer
//! up, actually looking an item up BY NAME once the right hashtable is
//! known) remain untranslated - they need `globvars_var`/`vimvars_var`
//! (whole-scope-as-a-single-item statics, still not built - see above),
//! `curbuf.b_bufvar`/`curwin.w_winvar`/`curtab.tp_winvar` fields (not
//! yet checked for existence), and `script_autoload` (real file I/O +
//! script sourcing, substantial on its own) - confirmed via direct
//! reading of their real bodies, not assumed; correctly left as a
//! separate future increment rather than folded into this one.
//!
//! Deferred: everything else in this file (variable get/set/unlet,
//! `:let` parsing, `evalvars_init`, etc.).

use crate::eval::typval::value_check_lock;
use crate::eval::typval_defs::{
    dict_item_flags, BoolVarValue, DictT, DictitemT, DictitemVariant, ScidT, ScopeDictDictItem,
    ScopeType, SpecialVarValue, TypvalT, TypvalValue, VarLockStatus, VarType, VarnumberT,
    DO_NOT_FREE_CNT,
};
use crate::eval::userfunc::{get_funccal_args_dict, get_funccal_local_dict};
use crate::hashtab::hashitem_empty;
use crate::hashtab_defs::HashtabT;
use crate::runtime_defs::ScriptvarT;

/// `-1`, matching `LuaRef`'s "no reference" convention already
/// established (e.g. `eval/typval.rs`'s own private `LUA_NOREF`).
const LUA_NOREF: crate::types_defs::LuaRef = -1;

/// Initialize `dict` as a scope dict and set `dict_var` to point to it
/// (`init_var_dict`).
///
/// `dict`/`dict_var` are typically two sibling fields of a larger,
/// heap-allocated struct (e.g. [`crate::runtime_defs::ScriptvarT`]'s
/// `sv_dict`/`sv_var`) - `dict_var` ends up storing a raw pointer to
/// `dict`'s own address, so callers must ensure `dict` does not move
/// in memory for as long as `dict_var` (or anything that copies its
/// `di_tv` value) remains reachable - the same requirement as any
/// other long-lived `*mut DictT` elsewhere in this crate.
pub fn init_var_dict(dict: &mut DictT, dict_var: &mut ScopeDictDictItem, scope: ScopeType) {
    dict.dv_hashtab = crate::hashtab_defs::HashtabT::hash_init();
    dict.dv_lock = VarLockStatus::Unlocked;
    dict.dv_scope = scope;
    dict.dv_refcount = DO_NOT_FREE_CNT;
    dict.dv_copy_id = 0;
    dict_var.di_tv.value = TypvalValue::Dict(dict as *mut DictT);
    dict_var.di_tv.v_lock = VarLockStatus::Fixed;
    dict_var.di_flags = dict_item_flags::RO | dict_item_flags::FIX;
    dict_var.di_key = vec![0]; // empty NUL-terminated key, matching di_key[0] = NUL
    // QUEUE_INIT(&dict->watchers) omitted - see this module's own doc
    // comment.
}

/// Allocate a new hashtab for a sourced script. It will be used while
/// sourcing this script and when executing functions defined in the
/// script (`new_script_vars`).
///
/// # Panics
/// Panics if `id` is out of range - see
/// [`crate::runtime::script_item`]'s own doc comment. In practice this
/// never happens: this function is only ever called by
/// `crate::runtime::new_script_item` immediately after allocating the
/// slot at `id`, exactly mirroring the original's own call site.
pub fn new_script_vars(id: ScidT) {
    let mut sv = Box::new(ScriptvarT {
        sv_var: ScopeDictDictItem::default(),
        // A fresh, zeroed DictT - matches the original's own
        // xcalloc(1, sizeof(scriptvar_T)), NOT tv_dict_alloc: a
        // script-scope dict has dv_refcount == DO_NOT_FREE_CNT (set
        // below by init_var_dict) and must NOT be linked into the
        // GC_FIRST_DICT used-dicts list (dv_used_next/dv_used_prev
        // stay null), matching the original exactly - it lives for
        // the whole session, never garbage collected via the normal
        // refcount path.
        sv_dict: DictT {
            dv_lock: VarLockStatus::Unlocked,
            dv_scope: ScopeType::NoScope,
            dv_refcount: 0,
            dv_copy_id: 0,
            dv_hashtab: crate::hashtab_defs::HashtabT::hash_init(),
            dv_index: std::collections::HashMap::new(),
            dv_copydict: std::ptr::null_mut(),
            dv_used_next: std::ptr::null_mut(),
            dv_used_prev: std::ptr::null_mut(),
            lua_table_ref: LUA_NOREF,
        },
    });
    init_var_dict(&mut sv.sv_dict, &mut sv.sv_var, ScopeType::Scope);
    let sv_ptr = Box::into_raw(sv);
    let item = crate::runtime::script_item(id);
    // SAFETY: item is a valid pointer to a live ScriptitemT - forwarded
    // from crate::runtime::script_item's own contract, guaranteed by
    // this function's own doc comment above (id is always freshly
    // allocated by runtime::new_script_item just before calling this).
    unsafe { (*item).sn_vars = sv_ptr };
}


/// Flags for `struct vimvar`'s own `vv_flags` field (`VV_COMPAT`/
/// `VV_RO`/`VV_RO_SBX`).
pub mod vv_flag {
    /// compatible, also used without the `"v:"` prefix (`VV_COMPAT`).
    pub const COMPAT: u8 = 1;
    /// read-only (`VV_RO`).
    pub const RO: u8 = 2;
    /// read-only in the sandbox (`VV_RO_SBX`).
    pub const RO_SBX: u8 = 4;
}

/// Defines for Vim variables (`VimVarIndex`, from `eval_defs.h`).
/// Mechanically transcribed from the header's own
/// `enum { VV_COUNT, VV_COUNT1, ... }` (108 values, in file order -
/// the enum's own declaration order, which the header assigns no
/// explicit numbers to, so each variant's discriminant here is simply
/// its position). Order is load-bearing: see this module's own doc
/// comment for how `VIMVARS` is indexed by this exact enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum VimVarIndex {
    /// `v:count` (VV_COUNT).
    Count = 0,
    /// `v:count1` (VV_COUNT1).
    Count1 = 1,
    /// `v:prevcount` (VV_PREVCOUNT).
    Prevcount = 2,
    /// `v:errmsg` (VV_ERRMSG).
    Errmsg = 3,
    /// `v:warningmsg` (VV_WARNINGMSG).
    Warningmsg = 4,
    /// `v:statusmsg` (VV_STATUSMSG).
    Statusmsg = 5,
    /// `v:shell_error` (VV_SHELL_ERROR).
    ShellError = 6,
    /// `v:this_session` (VV_THIS_SESSION).
    ThisSession = 7,
    /// `v:version` (VV_VERSION).
    Version = 8,
    /// `v:lnum` (VV_LNUM).
    Lnum = 9,
    /// `v:termrequest` (VV_TERMREQUEST).
    Termrequest = 10,
    /// `v:termresponse` (VV_TERMRESPONSE).
    Termresponse = 11,
    /// `v:fname` (VV_FNAME).
    Fname = 12,
    /// `v:lang` (VV_LANG).
    Lang = 13,
    /// `v:lc_time` (VV_LC_TIME).
    LcTime = 14,
    /// `v:ctype` (VV_CTYPE).
    Ctype = 15,
    /// `v:charconvert_from` (VV_CC_FROM).
    CcFrom = 16,
    /// `v:charconvert_to` (VV_CC_TO).
    CcTo = 17,
    /// `v:fname_in` (VV_FNAME_IN).
    FnameIn = 18,
    /// `v:fname_out` (VV_FNAME_OUT).
    FnameOut = 19,
    /// `v:fname_new` (VV_FNAME_NEW).
    FnameNew = 20,
    /// `v:fname_diff` (VV_FNAME_DIFF).
    FnameDiff = 21,
    /// `v:cmdarg` (VV_CMDARG).
    Cmdarg = 22,
    /// `v:foldstart` (VV_FOLDSTART).
    Foldstart = 23,
    /// `v:foldend` (VV_FOLDEND).
    Foldend = 24,
    /// `v:folddashes` (VV_FOLDDASHES).
    Folddashes = 25,
    /// `v:foldlevel` (VV_FOLDLEVEL).
    Foldlevel = 26,
    /// `v:progname` (VV_PROGNAME).
    Progname = 27,
    /// `v:servername` (VV_SEND_SERVER).
    SendServer = 28,
    /// `v:dying` (VV_DYING).
    Dying = 29,
    /// `v:exception` (VV_EXCEPTION).
    Exception = 30,
    /// `v:throwpoint` (VV_THROWPOINT).
    Throwpoint = 31,
    /// `v:register` (VV_REG).
    Reg = 32,
    /// `v:cmdbang` (VV_CMDBANG).
    Cmdbang = 33,
    /// `v:insertmode` (VV_INSERTMODE).
    Insertmode = 34,
    /// `v:val` (VV_VAL).
    Val = 35,
    /// `v:key` (VV_KEY).
    Key = 36,
    /// `v:profiling` (VV_PROFILING).
    Profiling = 37,
    /// `v:fcs_reason` (VV_FCS_REASON).
    FcsReason = 38,
    /// `v:fcs_choice` (VV_FCS_CHOICE).
    FcsChoice = 39,
    /// `v:beval_bufnr` (VV_BEVAL_BUFNR).
    BevalBufnr = 40,
    /// `v:beval_winnr` (VV_BEVAL_WINNR).
    BevalWinnr = 41,
    /// `v:beval_winid` (VV_BEVAL_WINID).
    BevalWinid = 42,
    /// `v:beval_lnum` (VV_BEVAL_LNUM).
    BevalLnum = 43,
    /// `v:beval_col` (VV_BEVAL_COL).
    BevalCol = 44,
    /// `v:beval_text` (VV_BEVAL_TEXT).
    BevalText = 45,
    /// `v:scrollstart` (VV_SCROLLSTART).
    Scrollstart = 46,
    /// `v:swapname` (VV_SWAPNAME).
    Swapname = 47,
    /// `v:swapchoice` (VV_SWAPCHOICE).
    Swapchoice = 48,
    /// `v:swapcommand` (VV_SWAPCOMMAND).
    Swapcommand = 49,
    /// `v:char` (VV_CHAR).
    Char = 50,
    /// `v:mouse_win` (VV_MOUSE_WIN).
    MouseWin = 51,
    /// `v:mouse_winid` (VV_MOUSE_WINID).
    MouseWinid = 52,
    /// `v:mouse_lnum` (VV_MOUSE_LNUM).
    MouseLnum = 53,
    /// `v:mouse_col` (VV_MOUSE_COL).
    MouseCol = 54,
    /// `v:operator` (VV_OP).
    Op = 55,
    /// `v:searchforward` (VV_SEARCHFORWARD).
    Searchforward = 56,
    /// `v:hlsearch` (VV_HLSEARCH).
    Hlsearch = 57,
    /// `v:oldfiles` (VV_OLDFILES).
    Oldfiles = 58,
    /// `v:windowid` (VV_WINDOWID).
    Windowid = 59,
    /// `v:progpath` (VV_PROGPATH).
    Progpath = 60,
    /// `v:completed_item` (VV_COMPLETED_ITEM).
    CompletedItem = 61,
    /// `v:option_new` (VV_OPTION_NEW).
    OptionNew = 62,
    /// `v:option_old` (VV_OPTION_OLD).
    OptionOld = 63,
    /// `v:option_oldlocal` (VV_OPTION_OLDLOCAL).
    OptionOldlocal = 64,
    /// `v:option_oldglobal` (VV_OPTION_OLDGLOBAL).
    OptionOldglobal = 65,
    /// `v:option_command` (VV_OPTION_COMMAND).
    OptionCommand = 66,
    /// `v:option_type` (VV_OPTION_TYPE).
    OptionType = 67,
    /// `v:errors` (VV_ERRORS).
    Errors = 68,
    /// `v:false` (VV_FALSE).
    False = 69,
    /// `v:true` (VV_TRUE).
    True = 70,
    /// `v:null` (VV_NULL).
    Null = 71,
    /// `v:numbermax` (VV_NUMBERMAX).
    Numbermax = 72,
    /// `v:numbermin` (VV_NUMBERMIN).
    Numbermin = 73,
    /// `v:numbersize` (VV_NUMBERSIZE).
    Numbersize = 74,
    /// `v:vim_did_enter` (VV_VIM_DID_ENTER).
    VimDidEnter = 75,
    /// `v:testing` (VV_TESTING).
    Testing = 76,
    /// `v:t_number` (VV_TYPE_NUMBER).
    TypeNumber = 77,
    /// `v:t_string` (VV_TYPE_STRING).
    TypeString = 78,
    /// `v:t_func` (VV_TYPE_FUNC).
    TypeFunc = 79,
    /// `v:t_list` (VV_TYPE_LIST).
    TypeList = 80,
    /// `v:t_dict` (VV_TYPE_DICT).
    TypeDict = 81,
    /// `v:t_float` (VV_TYPE_FLOAT).
    TypeFloat = 82,
    /// `v:t_bool` (VV_TYPE_BOOL).
    TypeBool = 83,
    /// `v:t_blob` (VV_TYPE_BLOB).
    TypeBlob = 84,
    /// `v:event` (VV_EVENT).
    Event = 85,
    /// `v:versionlong` (VV_VERSIONLONG).
    Versionlong = 86,
    /// `v:echospace` (VV_ECHOSPACE).
    Echospace = 87,
    /// `v:argf` (VV_ARGF).
    Argf = 88,
    /// `v:argv` (VV_ARGV).
    Argv = 89,
    /// `v:collate` (VV_COLLATE).
    Collate = 90,
    /// `v:exiting` (VV_EXITING).
    Exiting = 91,
    /// `v:maxcol` (VV_MAXCOL).
    Maxcol = 92,
    /// `v:stacktrace` (VV_STACKTRACE).
    Stacktrace = 93,
    /// `v:vim_did_init` (VV_VIM_DID_INIT).
    VimDidInit = 94,
    /// `v:stderr` (VV_STDERR).
    Stderr = 95,
    /// `v:msgpack_types` (VV_MSGPACK_TYPES).
    MsgpackTypes = 96,
    /// `v:_null_string` (VV__NULL_STRING).
    NullString = 97,
    /// `v:_null_list` (VV__NULL_LIST).
    NullList = 98,
    /// `v:_null_dict` (VV__NULL_DICT).
    NullDict = 99,
    /// `v:_null_blob` (VV__NULL_BLOB).
    NullBlob = 100,
    /// `v:lua` (VV_LUA).
    Lua = 101,
    /// `v:relnum` (VV_RELNUM).
    Relnum = 102,
    /// `v:virtnum` (VV_VIRTNUM).
    Virtnum = 103,
    /// `v:starttime` (VV_STARTTIME).
    Starttime = 104,
    /// `v:exitreason` (VV_EXITREASON).
    Exitreason = 105,
    /// `v:useractive` (VV_USERACTIVE).
    Useractive = 106,
    /// `v:startreason` (VV_STARTREASON).
    Startreason = 107,
}

/// One entry of the `v:` variable table (`struct vimvar` - `vv_name`/
/// `vv_flags`/`vv_di` in full, matching the original's embedded
/// `TV_DICTITEM_STRUCT(...) vv_di` exactly via a real, embedded
/// [`DictitemT`] rather than a side-table lookup - see [`VIMVARDICT`]'s
/// own doc comment for why this shape (as opposed to a separately
/// heap-allocated `DictitemT` per entry) is both safe and necessary).
struct Vimvar {
    /// Name of the variable, without `v:` (`vv_name`).
    name: &'static str,
    /// Flags: some combination of [`vv_flag::COMPAT`]/
    /// [`vv_flag::RO`]/[`vv_flag::RO_SBX`] (`vv_flags`).
    flags: u8,
    /// Value, lock status, `di_flags`, and `di_key` (`vv_di`). `di_key`/
    /// `di_flags` are left at their empty/zero defaults in this array's
    /// own literal below - [`VIMVARS`]'s construction fills them in
    /// right afterward, in one pass derived from `name`/`flags`
    /// (mirroring `evalvars_init`'s own per-entry `di_flags` assignment,
    /// `vars.c` lines 269-277, and the `VV()` macro's compile-time
    /// `.di_key = name` initializer).
    di: DictitemT,
}

/// The `v:` variable table (`vimvars[]`). See this module's own doc
/// comment for the full explanation of this table's construction,
/// indexing, and relationship to [`evalvars_init`].
static VIMVARS: std::sync::LazyLock<crate::globals::GlobalCell<Vec<Vimvar>>> =
    std::sync::LazyLock::new(|| {
        let mut vimvars = vec![
    // VV_COUNT
    Vimvar { name: "count", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_COUNT1
    Vimvar { name: "count1", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_PREVCOUNT
    Vimvar { name: "prevcount", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_ERRMSG
    Vimvar { name: "errmsg", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_WARNINGMSG
    Vimvar { name: "warningmsg", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_STATUSMSG
    Vimvar { name: "statusmsg", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_SHELL_ERROR
    Vimvar { name: "shell_error", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_THIS_SESSION
    Vimvar { name: "this_session", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_VERSION
    Vimvar { name: "version", flags: vv_flag::COMPAT | vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_LNUM
    Vimvar { name: "lnum", flags: vv_flag::RO_SBX, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_TERMREQUEST
    Vimvar { name: "termrequest", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_TERMRESPONSE
    Vimvar { name: "termresponse", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_FNAME
    Vimvar { name: "fname", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_LANG
    Vimvar { name: "lang", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_LC_TIME
    Vimvar { name: "lc_time", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_CTYPE
    Vimvar { name: "ctype", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_CC_FROM
    Vimvar { name: "charconvert_from", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_CC_TO
    Vimvar { name: "charconvert_to", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_FNAME_IN
    Vimvar { name: "fname_in", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_FNAME_OUT
    Vimvar { name: "fname_out", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_FNAME_NEW
    Vimvar { name: "fname_new", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_FNAME_DIFF
    Vimvar { name: "fname_diff", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_CMDARG
    Vimvar { name: "cmdarg", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_FOLDSTART
    Vimvar { name: "foldstart", flags: vv_flag::RO_SBX, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_FOLDEND
    Vimvar { name: "foldend", flags: vv_flag::RO_SBX, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_FOLDDASHES
    Vimvar { name: "folddashes", flags: vv_flag::RO_SBX, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_FOLDLEVEL
    Vimvar { name: "foldlevel", flags: vv_flag::RO_SBX, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_PROGNAME
    Vimvar { name: "progname", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_SEND_SERVER
    Vimvar { name: "servername", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_DYING
    Vimvar { name: "dying", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_EXCEPTION
    Vimvar { name: "exception", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_THROWPOINT
    Vimvar { name: "throwpoint", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_REG
    Vimvar { name: "register", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_CMDBANG
    Vimvar { name: "cmdbang", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_INSERTMODE
    Vimvar { name: "insertmode", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_VAL
    Vimvar { name: "val", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Unknown }, di_flags: 0, di_key: Vec::new() } },
    // VV_KEY
    Vimvar { name: "key", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Unknown }, di_flags: 0, di_key: Vec::new() } },
    // VV_PROFILING
    Vimvar { name: "profiling", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_FCS_REASON
    Vimvar { name: "fcs_reason", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_FCS_CHOICE
    Vimvar { name: "fcs_choice", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_BEVAL_BUFNR
    Vimvar { name: "beval_bufnr", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_BEVAL_WINNR
    Vimvar { name: "beval_winnr", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_BEVAL_WINID
    Vimvar { name: "beval_winid", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_BEVAL_LNUM
    Vimvar { name: "beval_lnum", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_BEVAL_COL
    Vimvar { name: "beval_col", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_BEVAL_TEXT
    Vimvar { name: "beval_text", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_SCROLLSTART
    Vimvar { name: "scrollstart", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_SWAPNAME
    Vimvar { name: "swapname", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_SWAPCHOICE
    Vimvar { name: "swapchoice", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_SWAPCOMMAND
    Vimvar { name: "swapcommand", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_CHAR
    Vimvar { name: "char", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_MOUSE_WIN
    Vimvar { name: "mouse_win", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_MOUSE_WINID
    Vimvar { name: "mouse_winid", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_MOUSE_LNUM
    Vimvar { name: "mouse_lnum", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_MOUSE_COL
    Vimvar { name: "mouse_col", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_OP
    Vimvar { name: "operator", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_SEARCHFORWARD
    Vimvar { name: "searchforward", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_HLSEARCH
    Vimvar { name: "hlsearch", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_OLDFILES
    Vimvar { name: "oldfiles", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) }, di_flags: 0, di_key: Vec::new() } },
    // VV_WINDOWID
    Vimvar { name: "windowid", flags: vv_flag::RO_SBX, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_PROGPATH
    Vimvar { name: "progpath", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_COMPLETED_ITEM
    Vimvar { name: "completed_item", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(std::ptr::null_mut()) }, di_flags: 0, di_key: Vec::new() } },
    // VV_OPTION_NEW
    Vimvar { name: "option_new", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_OPTION_OLD
    Vimvar { name: "option_old", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_OPTION_OLDLOCAL
    Vimvar { name: "option_oldlocal", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_OPTION_OLDGLOBAL
    Vimvar { name: "option_oldglobal", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_OPTION_COMMAND
    Vimvar { name: "option_command", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_OPTION_TYPE
    Vimvar { name: "option_type", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_ERRORS
    Vimvar { name: "errors", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) }, di_flags: 0, di_key: Vec::new() } },
    // VV_FALSE
    Vimvar { name: "false", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Bool(BoolVarValue::False) }, di_flags: 0, di_key: Vec::new() } },
    // VV_TRUE
    Vimvar { name: "true", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Bool(BoolVarValue::False) }, di_flags: 0, di_key: Vec::new() } },
    // VV_NULL
    Vimvar { name: "null", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Special(SpecialVarValue::Null) }, di_flags: 0, di_key: Vec::new() } },
    // VV_NUMBERMAX
    Vimvar { name: "numbermax", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_NUMBERMIN
    Vimvar { name: "numbermin", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_NUMBERSIZE
    Vimvar { name: "numbersize", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_VIM_DID_ENTER
    Vimvar { name: "vim_did_enter", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_TESTING
    Vimvar { name: "testing", flags: 0, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_TYPE_NUMBER
    Vimvar { name: "t_number", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_TYPE_STRING
    Vimvar { name: "t_string", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_TYPE_FUNC
    Vimvar { name: "t_func", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_TYPE_LIST
    Vimvar { name: "t_list", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_TYPE_DICT
    Vimvar { name: "t_dict", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_TYPE_FLOAT
    Vimvar { name: "t_float", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_TYPE_BOOL
    Vimvar { name: "t_bool", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_TYPE_BLOB
    Vimvar { name: "t_blob", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_EVENT
    Vimvar { name: "event", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(std::ptr::null_mut()) }, di_flags: 0, di_key: Vec::new() } },
    // VV_VERSIONLONG
    Vimvar { name: "versionlong", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_ECHOSPACE
    Vimvar { name: "echospace", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_ARGF
    Vimvar { name: "argf", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) }, di_flags: 0, di_key: Vec::new() } },
    // VV_ARGV
    Vimvar { name: "argv", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) }, di_flags: 0, di_key: Vec::new() } },
    // VV_COLLATE
    Vimvar { name: "collate", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_EXITING
    Vimvar { name: "exiting", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_MAXCOL
    Vimvar { name: "maxcol", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_STACKTRACE
    Vimvar { name: "stacktrace", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) }, di_flags: 0, di_key: Vec::new() } },
    // VV_VIM_DID_INIT
    Vimvar { name: "vim_did_init", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_STDERR
    Vimvar { name: "stderr", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_MSGPACK_TYPES
    Vimvar { name: "msgpack_types", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(std::ptr::null_mut()) }, di_flags: 0, di_key: Vec::new() } },
    // VV__NULL_STRING
    Vimvar { name: "_null_string", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV__NULL_LIST
    Vimvar { name: "_null_list", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) }, di_flags: 0, di_key: Vec::new() } },
    // VV__NULL_DICT
    Vimvar { name: "_null_dict", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(std::ptr::null_mut()) }, di_flags: 0, di_key: Vec::new() } },
    // VV__NULL_BLOB
    Vimvar { name: "_null_blob", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Blob(std::ptr::null_mut()) }, di_flags: 0, di_key: Vec::new() } },
    // VV_LUA
    Vimvar { name: "lua", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Partial(std::ptr::null_mut()) }, di_flags: 0, di_key: Vec::new() } },
    // VV_RELNUM
    Vimvar { name: "relnum", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_VIRTNUM
    Vimvar { name: "virtnum", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_STARTTIME
    Vimvar { name: "starttime", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_EXITREASON
    Vimvar { name: "exitreason", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
    // VV_USERACTIVE
    Vimvar { name: "useractive", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) }, di_flags: 0, di_key: Vec::new() } },
    // VV_STARTREASON
    Vimvar { name: "startreason", flags: vv_flag::RO, di: DictitemT { di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) }, di_flags: 0, di_key: Vec::new() } },
        ];
        // Mirrors evalvars_init's own per-entry di_flags assignment
        // (vars.c lines 271-277) and the VV() macro's compile-time
        // `.di_key = name` initializer - both derived purely from each
        // entry's own name/flags, so a single deterministic pass here
        // is equivalent to (not just an approximation of) the
        // original's static-initializer-plus-evalvars_init sequence.
        for v in &mut vimvars {
            v.di.di_flags = if v.flags & vv_flag::RO != 0 {
                dict_item_flags::RO | dict_item_flags::FIX
            } else if v.flags & vv_flag::RO_SBX != 0 {
                dict_item_flags::RO_SBX | dict_item_flags::FIX
            } else {
                dict_item_flags::FIX
            };
            v.di.di_key.clear();
            v.di.di_key.extend_from_slice(v.name.as_bytes());
            v.di.di_key.push(0); // NUL terminator, matching di_key's usual contract
        }
        crate::globals::GlobalCell::new(vimvars)
    });

/// The global (`g:`) scope dict (`globvardict`; `globvarht` is just
/// `globvardict.dv_hashtab` in the original, via a `#define`).
///
/// A `dict_T` file-static in the original - never heap-allocated,
/// hence never linked into `GC_FIRST_DICT`'s used-dicts list, matching
/// `new_script_vars`'s own already-established precedent for the
/// analogous script-scope dict (see its doc comment in
/// `crate::runtime`). `dv_scope`/`dv_refcount` now match
/// `evalvars_init`'s own `init_var_dict(get_globvar_dict(),
/// &globvars_var, VAR_DEF_SCOPE)` call's real effect on the DICT
/// itself (`DefScope`/`DO_NOT_FREE_CNT`) - added alongside
/// `GLOBVARS_VAR` below, since both represent the same slice of
/// `init_var_dict`'s work. `evalvars_init` ITSELF is now fully
/// translated (see its own doc comment), but its real body only ever
/// touches `v:`-side globals, never anything `g:`-specific - so it has
/// no further bearing on `GLOBVARDICT` beyond the `init_var_dict`
/// effect already accounted for here.
static GLOBVARDICT: std::sync::LazyLock<crate::globals::GlobalCell<DictT>> =
    std::sync::LazyLock::new(|| {
        crate::globals::GlobalCell::new(DictT {
            dv_lock: VarLockStatus::Unlocked,
            dv_scope: ScopeType::DefScope,
            dv_refcount: DO_NOT_FREE_CNT,
            dv_copy_id: 0,
            dv_hashtab: crate::hashtab_defs::HashtabT::hash_init(),
            dv_index: std::collections::HashMap::new(),
            dv_copydict: std::ptr::null_mut(),
            dv_used_next: std::ptr::null_mut(),
            dv_used_prev: std::ptr::null_mut(),
            lua_table_ref: LUA_NOREF,
        })
    });

/// The `globvars_var` file-static - the whole `g:` scope, "as if it
/// were one `dictitem_T`" (really a [`ScopeDictDictItem`], per
/// [`DictitemVariant`]'s own doc comment). Only ever consumed via
/// [`find_var_in_ht`]'s own `varname_len == 0` (implicit whole-scope)
/// branch, matching the original's sole real use
/// (`(dictitem_T *)&globvars_var`). Kept private, matching the
/// original's own file-static visibility (`globvars_var` is never
/// referenced outside `vars.c` either).
static GLOBVARS_VAR: std::sync::LazyLock<crate::globals::GlobalCell<ScopeDictDictItem>> =
    std::sync::LazyLock::new(|| {
        crate::globals::GlobalCell::new(ScopeDictDictItem {
            di_tv: TypvalT { v_lock: VarLockStatus::Fixed, value: TypvalValue::Dict(get_globvar_dict()) },
            di_flags: dict_item_flags::RO | dict_item_flags::FIX,
            di_key: vec![0], // empty NUL-terminated key, matching di_key[0] = NUL
        })
    });

/// @return the global (`g:`) variable dictionary (`get_globvar_dict`).
///
/// Uses `GLOBVARDICT.as_ptr()` (not `.get_mut()`) so that the returned
/// pointer remains valid across ANY later, independent call to this
/// same function - `.get_mut()` creates a fresh exclusive reference on
/// every call, which (proven for real by Miri's Tree Borrows checker
/// against `VIMVARDICT`'s own analogous bug, fixed alongside this one)
/// would otherwise invalidate a previously-returned pointer the
/// moment `get_globvar_dict()` is called again from anywhere else -
/// exactly what happens whenever a caller holds onto an earlier
/// result across a call to another function (e.g. `del_menutrans_vars`)
/// that itself calls `get_globvar_dict()` again internally.
#[must_use]
pub fn get_globvar_dict() -> *mut DictT {
    // SAFETY: GLOBVARDICT is only ever read/written through this
    // module's own functions.
    GLOBVARDICT.as_ptr()
}

/// @return the global (`g:`) variable hash table (`get_globvar_ht`).
#[must_use]
pub fn get_globvar_ht() -> *mut HashtabT {
    // SAFETY: forwarded from get_globvar_dict's own established
    // convention.
    unsafe { &mut (*get_globvar_dict()).dv_hashtab as *mut HashtabT }
}


/// Delete all `"menutrans_"`-prefixed global variables
/// (`del_menutrans_vars`).
///
/// Unlike the original (locks `globvarht`, walks it via
/// `HASHTAB_ITER`, calling the small file-static `delete_var(ht, hi)`
/// per match), filters `GLOBVARDICT`'s own `dv_index` directly - no
/// hashtab traversal/locking needed, matching `vars_clear_ext`'s own
/// established precedent - and calls the already-real
/// [`crate::eval::typval::tv_dict_item_remove`] per match, which is
/// functionally identical to the original's own `delete_var` (both:
/// remove from the hashtab, clear the value, free the item shell) -
/// so no separate `delete_var` binding is needed here.
pub fn del_menutrans_vars() {
    // SAFETY: get_globvar_dict() (as_ptr()-based) never creates a
    // reference, so this is safe to call regardless of any other live
    // pointer into GLOBVARDICT elsewhere.
    let d = unsafe { &mut *get_globvar_dict() };
    let items: Vec<*mut DictitemT> = d
        .dv_index
        .values()
        .copied()
        .filter(|&item| {
            // SAFETY: every dv_index entry is a live DictitemT
            // pointer, populated/depopulated in lockstep with
            // dv_hashtab by this module's own functions.
            unsafe { (*item).di_key.starts_with(b"menutrans_") }
        })
        .collect();
    for item in items {
        // SAFETY: item was just looked up from d's own dv_index,
        // satisfying tv_dict_item_remove's own safety contract.
        unsafe { crate::eval::typval::tv_dict_item_remove(d, item) };
    }
}

/// All recognized msgpack types (`MessagePackType`, `eval_defs.h`).
///
/// The only real, currently-translated consumer is [`evalvars_init`]'s
/// own construction of `v:msgpack_types`/`EVAL_MSGPACK_TYPE_LISTS` -
/// the not-yet-translated `eval/decode.c`'s `msgpack_list_to_tv` and
/// `eval/encode.c`'s/`typval_encode.c.h`'s own msgpack-special-value
/// recognition are this enum's OTHER real consumers in the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum MessagePackType {
    Nil = 0,
    Boolean = 1,
    Integer = 2,
    Float = 3,
    String = 4,
    Array = 5,
    Map = 6,
    Ext = 7,
}

/// Number of msgpack types (`NUM_MSGPACK_TYPES`, `eval/vars.h`).
pub const NUM_MSGPACK_TYPES: usize = 8;

/// Display names for each `MessagePackType`, indexed by its own
/// discriminant (`msgpack_type_names`, `eval/vars.c`).
const MSGPACK_TYPE_NAMES: [&[u8]; NUM_MSGPACK_TYPES] =
    [b"nil", b"boolean", b"integer", b"float", b"string", b"array", b"map", b"ext"];

/// Array mapping each msgpack type (see `MessagePackType`) to its
/// corresponding sentinel `List` pointer inside `v:msgpack_types`
/// (`eval_msgpack_type_lists`, `eval/vars.c`/`eval/vars.h`) - populated
/// once by [`evalvars_init`], consumed by the not-yet-translated
/// `eval/decode.c`/`eval/encode.c` msgpack-special-value machinery to
/// recognize/produce these sentinel lists later. All null until
/// `evalvars_init` runs (matching the original's own static-
/// initializer-all-`NULL` state before its own `evalvars_init` call).
static EVAL_MSGPACK_TYPE_LISTS: std::sync::LazyLock<
    crate::globals::GlobalCell<[*mut crate::eval::typval_defs::ListT; NUM_MSGPACK_TYPES]>,
> = std::sync::LazyLock::new(|| crate::globals::GlobalCell::new([std::ptr::null_mut(); NUM_MSGPACK_TYPES]));

/// Set every `v:` special variable to its real startup default
/// (`evalvars_init`, `eval/vars.c`).
///
/// The `init_var_dict`/`vimvars[]`-population half of the original's
/// own job is ALREADY done: `GLOBVARDICT`/`VIMVARDICT`'s own lazy
/// construction already sets `dv_scope`/`dv_lock`/`dv_refcount` to
/// exactly what `init_var_dict` would (see their own doc comments),
/// and `VIMVARS`'s own construction already fills every entry's
/// `di_flags`/`di_key` from its `name`/`flags` (mirroring the
/// original's own per-entry loop). This function's OWN remaining real
/// job is overriding several `VIMVARS` entries' bare static-initializer
/// VALUES with real runtime startup values.
///
/// `v:version`/`v:versionlong` are now real too, via
/// `crate::version::min_vim_version`/`highest_patch` (previously
/// deferred pending those two functions, which now exist). The
/// `v:startreason` env-var-based OVERRIDE (checking `ENV_STARTREASON`
/// for a `nvim --remote`-triggered restart) is also now real, via
/// `crate::os::env::os_getenv`/`os_env_exists`/`os_unsetenv` (all
/// already existed - this was a stale deferral note, not a genuine
/// blocker). `v:msgpack_types` is now real too - its own construction
/// only needed the small `MessagePackType` enum/`MSGPACK_TYPE_NAMES`
/// array/`EVAL_MSGPACK_TYPE_LISTS` above, NOT the whole not-yet-
/// translated `eval/decode.c`/`eval/encode.c` JSON/msgpack encoding
/// subsystem (another stale deferral note - those files' OWN, separate
/// use of `eval_msgpack_type_lists` remains untranslated, but that
/// doesn't block THIS function's own job of just constructing the
/// array and the dict).
///
/// This means `evalvars_init` is now translated IN FULL - every single
/// `set_vim_var_*` call in the real C function's own body has a real
/// Rust equivalent above, with no remaining deferred pieces.
///
/// # Safety
/// Touches `crate::globals::GLOBALS` and the shared `VIMVARDICT`/
/// `VIMVARS`/`FUNC_HASHTAB` state - any TEST calling this must hold
/// `crate::globals::global_state_test_lock()` for its whole body.
pub unsafe fn evalvars_init() {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        let vim_version = crate::version::min_vim_version();
        set_vim_var_nr(VimVarIndex::Version, i64::from(vim_version));
        set_vim_var_nr(
            VimVarIndex::Versionlong,
            i64::from(vim_version) * 10000 + i64::from(crate::version::highest_patch()),
        );

        // v:msgpack_types: a real Fixed-locked dict of 8 Fixed-locked,
        // empty sentinel Lists, one per MessagePackType - each List's
        // own pointer is ALSO recorded in EVAL_MSGPACK_TYPE_LISTS,
        // matching the original's own eval_msgpack_type_lists[] array,
        // for the not-yet-translated eval/decode.c/eval/encode.c
        // msgpack-special-value machinery to recognize later.
        let msgpack_types_dict = crate::eval::typval::tv_dict_alloc();
        {
            let msgpack_lists = EVAL_MSGPACK_TYPE_LISTS.get_mut();
            for i in 0..NUM_MSGPACK_TYPES {
                let type_list = crate::eval::typval::tv_list_alloc(0);
                crate::eval::typval::tv_list_set_lock(type_list, VarLockStatus::Fixed);
                crate::eval::typval::tv_list_ref(type_list);
                let di = crate::eval::typval::tv_dict_item_alloc(MSGPACK_TYPE_NAMES[i]);
                (*di).di_flags |= dict_item_flags::RO | dict_item_flags::FIX;
                (*di).di_tv.value = TypvalValue::List(type_list);
                assert_ne!(
                    crate::eval::typval::tv_dict_add(&mut *msgpack_types_dict, di),
                    crate::vim_defs::FAIL,
                    "there must not be duplicate items in this dictionary by definition"
                );
                msgpack_lists[i] = type_list;
            }
        }
        (*msgpack_types_dict).dv_lock = VarLockStatus::Fixed;
        set_vim_var_dict(VimVarIndex::MsgpackTypes, msgpack_types_dict);

        set_vim_var_dict(VimVarIndex::CompletedItem, crate::eval::typval::tv_dict_alloc_lock(VarLockStatus::Fixed));
        set_vim_var_dict(VimVarIndex::Event, crate::eval::typval::tv_dict_alloc_lock(VarLockStatus::Fixed));

        let errors = crate::eval::typval::tv_list_alloc(crate::eval::typval_defs::ListLenSpecials::Unknown as isize);
        set_vim_var_list(VimVarIndex::Errors, errors);

        set_vim_var_nr(VimVarIndex::Stderr, 2); // CHAN_STDERR
        set_vim_var_nr(VimVarIndex::Searchforward, 1);
        set_vim_var_nr(VimVarIndex::Hlsearch, 1);
        set_vim_var_nr(VimVarIndex::Count1, 1);
        set_vim_var_string(VimVarIndex::Startreason, Some(b"normal"));
        set_vim_var_special(VimVarIndex::Exiting, crate::eval::typval_defs::SpecialVarValue::Null);

        set_vim_var_nr(VimVarIndex::TypeNumber, i64::from(crate::eval::typval_defs::var_type_result::NUMBER));
        set_vim_var_nr(VimVarIndex::TypeString, i64::from(crate::eval::typval_defs::var_type_result::STRING));
        set_vim_var_nr(VimVarIndex::TypeFunc, i64::from(crate::eval::typval_defs::var_type_result::FUNC));
        set_vim_var_nr(VimVarIndex::TypeList, i64::from(crate::eval::typval_defs::var_type_result::LIST));
        set_vim_var_nr(VimVarIndex::TypeDict, i64::from(crate::eval::typval_defs::var_type_result::DICT));
        set_vim_var_nr(VimVarIndex::TypeFloat, i64::from(crate::eval::typval_defs::var_type_result::FLOAT));
        set_vim_var_nr(VimVarIndex::TypeBool, i64::from(crate::eval::typval_defs::var_type_result::BOOL));
        set_vim_var_nr(VimVarIndex::TypeBlob, i64::from(crate::eval::typval_defs::var_type_result::BLOB));

        set_vim_var_bool(VimVarIndex::False, crate::eval::typval_defs::BoolVarValue::False);
        set_vim_var_bool(VimVarIndex::True, crate::eval::typval_defs::BoolVarValue::True);
        set_vim_var_special(VimVarIndex::Null, crate::eval::typval_defs::SpecialVarValue::Null);
        set_vim_var_nr(VimVarIndex::Numbermax, crate::eval::typval_defs::VARNUMBER_MAX);
        set_vim_var_nr(VimVarIndex::Numbermin, crate::eval::typval_defs::VARNUMBER_MIN);
        set_vim_var_nr(
            VimVarIndex::Numbersize,
            i64::from(std::mem::size_of::<crate::eval::typval_defs::VarnumberT>() as u32 * 8),
        );
        set_vim_var_nr(VimVarIndex::Maxcol, i64::from(crate::pos_defs::MAXCOL));

        let sc_col = crate::globals::GLOBALS.get_mut().sc_col;
        set_vim_var_nr(VimVarIndex::Echospace, i64::from(sc_col - 1));

        // v:lua: a bound, always-present Partial referencing no real
        // function (matching the original's own "shouldn't be printed,
        // but if it is, do not crash" placeholder name).
        let vvlua_partial = Box::into_raw(Box::new(crate::eval::typval_defs::PartialT {
            pt_name: Some(Vec::new()),
            pt_refcount: 1,
            ..Default::default()
        }));
        set_vim_var_partial(VimVarIndex::Lua, vvlua_partial);

        set_reg_var(0); // default for v:register is not 0 but '"'

        // Set v:startreason via environment variable (a real `nvim
        // --remote`-triggered restart) - os_getenv_noalloc's own
        // non-allocating pointer return is just a C memory-management
        // detail this crate's already-Vec-returning os_getenv doesn't
        // need to replicate.
        if let Some(startreason) =
            crate::os::env::os_getenv(crate::os::os::ENV_STARTREASON.as_bytes())
        {
            if startreason == b"restart!" || startreason == b"restart" {
                set_vim_var_string(VimVarIndex::Startreason, Some(&startreason));
            }
        }
        if crate::os::env::os_env_exists(crate::os::os::ENV_STARTREASON.as_bytes(), false) {
            crate::os::env::os_unsetenv(crate::os::os::ENV_STARTREASON.as_bytes());
        }
    }
}

#[cfg(test)]
mod evalvars_init_tests {
    use super::*;

    /// Verifies every real value `evalvars_init` sets, then explicitly
    /// releases the 4 heap-allocated resources it creates (2 `Dict`s,
    /// 1 `List`, 1 `Partial`), resetting each `VIMVARS` slot's stored
    /// pointer back to null afterward - this crate's own established
    /// convention for testing an intentionally-one-shot session
    /// bootstrap function (matching `new_script_vars`/`func_hashtab`'s
    /// own precedent) without leaving a dangling pointer OR a
    /// permanently-populated `GC_FIRST_DICT`/`GC_FIRST_LIST` entry for
    /// later, unrelated tests to trip over.
    ///
    /// Deliberately does NOT reset the simple scalar slots
    /// (`v:stderr`, `v:searchforward`, `v:hlsearch`, `v:count1`,
    /// `v:startreason`, `v:exiting`, the 8 `v:t_*` constants,
    /// `v:false`/`v:true`/`v:null`, `v:numbermax`/`v:numbermin`/
    /// `v:numbersize`/`v:maxcol`/`v:echospace`, `v:register`) back to
    /// their pre-call values: none of them hold a heap-linked resource
    /// that could dangle, and no OTHER test in this file asserts a
    /// SPECIFIC value for any of these slots without first setting it
    /// itself (verified by inspection) - only var_type()-only checks
    /// exist, which stay correct regardless (evalvars_init never
    /// changes a slot's TYPE tag, only its VALUE).
    #[test]
    fn evalvars_init_sets_real_startup_values() {
        let _lock = crate::globals::global_state_test_lock();

        // Precondition, matching this crate's own established
        // GC_FIRST_DICT/GC_FIRST_LIST convention: nothing else should
        // be live before this test runs its own allocations.
        assert!(crate::eval::typval::gc_first_dict_is_empty());
        assert!(crate::eval::typval::gc_first_list_is_empty());

        // Captured BEFORE the call so this assertion is correct
        // regardless of what any other test has left GLOBALS.sc_col
        // at - asserting the RELATIONSHIP evalvars_init computes, not
        // a hardcoded absolute value.
        let sc_col_before = unsafe { crate::globals::GLOBALS.get_mut() }.sc_col;

        // SAFETY: this test holds global_state_test_lock() for its
        // whole body, matching every other GLOBALS/VIMVARS-touching
        // test in this file.
        unsafe {
            evalvars_init();

            // v:completed_item / v:event: real, empty, Fixed-locked
            // dicts, and genuinely distinct from one another.
            let completed_item = get_vim_var_dict(VimVarIndex::CompletedItem);
            assert!(!completed_item.is_null());
            assert_eq!((*completed_item).dv_lock, VarLockStatus::Fixed);
            assert_eq!((*completed_item).dv_index.len(), 0);

            let event = get_vim_var_dict(VimVarIndex::Event);
            assert!(!event.is_null());
            assert_eq!((*event).dv_lock, VarLockStatus::Fixed);
            assert_eq!((*event).dv_index.len(), 0);
            assert_ne!(completed_item, event);

            // v:errors: a real, empty list.
            let errors = get_vim_var_list(VimVarIndex::Errors);
            assert!(!errors.is_null());
            assert_eq!((*errors).lv_len, 0);

            // v:version / v:versionlong: real min_vim_version()/
            // highest_patch()-derived values (801 / 8012424 in this
            // checkout, per version.rs's own VIM_VERSIONS[0]/
            // HIGHEST_PATCH constants).
            assert_eq!(get_vim_var_nr(VimVarIndex::Version), 801);
            assert_eq!(get_vim_var_nr(VimVarIndex::Versionlong), 8_012_424);

            // v:msgpack_types: a real, Fixed-locked dict of 8 real,
            // Fixed-locked, empty sentinel Lists (one per
            // MessagePackType), each ALSO recorded in
            // EVAL_MSGPACK_TYPE_LISTS at the matching index.
            let msgpack_types = get_vim_var_dict(VimVarIndex::MsgpackTypes);
            assert!(!msgpack_types.is_null());
            assert_eq!((*msgpack_types).dv_lock, VarLockStatus::Fixed);
            assert_eq!((*msgpack_types).dv_index.len(), NUM_MSGPACK_TYPES);
            for (i, name) in MSGPACK_TYPE_NAMES.iter().enumerate() {
                let item = crate::eval::typval::tv_dict_find(Some(&mut *msgpack_types), name)
                    .expect("every msgpack type name should be present");
                assert_eq!(
                    (*item).di_flags & (dict_item_flags::RO | dict_item_flags::FIX),
                    dict_item_flags::RO | dict_item_flags::FIX
                );
                match (*item).di_tv.value {
                    TypvalValue::List(l) => {
                        assert!(!l.is_null());
                        assert_eq!((*l).lv_len, 0);
                        assert_eq!((*l).lv_lock, VarLockStatus::Fixed);
                        assert_eq!((*l).lv_refcount, 1);
                        assert_eq!(EVAL_MSGPACK_TYPE_LISTS.get_mut()[i], l);
                    }
                    _ => panic!("expected a List-typed msgpack type entry"),
                }
            }

            // Simple numeric/string/special values.
            assert_eq!(get_vim_var_nr(VimVarIndex::Stderr), 2);
            assert_eq!(get_vim_var_nr(VimVarIndex::Searchforward), 1);
            assert_eq!(get_vim_var_nr(VimVarIndex::Hlsearch), 1);
            assert_eq!(get_vim_var_nr(VimVarIndex::Count1), 1);
            assert_eq!(get_vim_var_str(VimVarIndex::Startreason), b"normal".to_vec());
            assert_eq!(
                (*get_vim_var_tv(VimVarIndex::Exiting)).value,
                TypvalValue::Special(SpecialVarValue::Null)
            );

            // v:t_* type constants (var_type_result's own numbering,
            // NOT VarType's discriminants - a deliberately separate
            // scheme, see var_type_result's own doc comment).
            use crate::eval::typval_defs::var_type_result;
            assert_eq!(get_vim_var_nr(VimVarIndex::TypeNumber), i64::from(var_type_result::NUMBER));
            assert_eq!(get_vim_var_nr(VimVarIndex::TypeString), i64::from(var_type_result::STRING));
            assert_eq!(get_vim_var_nr(VimVarIndex::TypeFunc), i64::from(var_type_result::FUNC));
            assert_eq!(get_vim_var_nr(VimVarIndex::TypeList), i64::from(var_type_result::LIST));
            assert_eq!(get_vim_var_nr(VimVarIndex::TypeDict), i64::from(var_type_result::DICT));
            assert_eq!(get_vim_var_nr(VimVarIndex::TypeFloat), i64::from(var_type_result::FLOAT));
            assert_eq!(get_vim_var_nr(VimVarIndex::TypeBool), i64::from(var_type_result::BOOL));
            assert_eq!(get_vim_var_nr(VimVarIndex::TypeBlob), i64::from(var_type_result::BLOB));

            // v:false / v:true / v:null.
            assert_eq!((*get_vim_var_tv(VimVarIndex::False)).value, TypvalValue::Bool(BoolVarValue::False));
            assert_eq!((*get_vim_var_tv(VimVarIndex::True)).value, TypvalValue::Bool(BoolVarValue::True));
            assert_eq!((*get_vim_var_tv(VimVarIndex::Null)).value, TypvalValue::Special(SpecialVarValue::Null));

            // v:numbermax / v:numbermin / v:numbersize / v:maxcol /
            // v:echospace.
            assert_eq!(get_vim_var_nr(VimVarIndex::Numbermax), crate::eval::typval_defs::VARNUMBER_MAX);
            assert_eq!(get_vim_var_nr(VimVarIndex::Numbermin), crate::eval::typval_defs::VARNUMBER_MIN);
            assert_eq!(get_vim_var_nr(VimVarIndex::Numbersize), 64);
            assert_eq!(get_vim_var_nr(VimVarIndex::Maxcol), i64::from(crate::pos_defs::MAXCOL));
            assert_eq!(get_vim_var_nr(VimVarIndex::Echospace), i64::from(sc_col_before - 1));

            // v:lua: a real, bound Partial (empty name, not None -
            // matching the original's own "shouldn't be printed, but
            // if it is, do not crash" placeholder).
            let lua_partial = get_vim_var_partial(VimVarIndex::Lua);
            assert!(!lua_partial.is_null());
            assert_eq!((*lua_partial).pt_refcount, 1);
            assert_eq!((*lua_partial).pt_name, Some(Vec::new()));

            // v:register: reflects set_reg_var(0)'s effect (the
            // default register name is '"', not the literal digit 0).
            assert_eq!(get_vim_var_str(VimVarIndex::Reg), vec![b'"']);

            // --- Cleanup: release every heap-allocated resource this
            // call created, then reset each VIMVARS slot's own stored
            // pointer back to null - avoiding both a permanent
            // GC_FIRST_DICT/GC_FIRST_LIST entry and a dangling pointer
            // left in VIMVARS for a later test to dereference.
            //
            // tv_dict_unref(msgpack_types) also releases all 8 of its
            // own List-typed items (via tv_dict_free_contents's own
            // per-item tv_clear_simple call, which calls tv_list_unref
            // on a List value) - dropping each list's refcount from 1
            // (set by the earlier tv_list_ref) to 0, freeing it. Reset
            // EVAL_MSGPACK_TYPE_LISTS to all-null afterward too, to
            // avoid leaving 8 dangling pointers of its own.
            crate::eval::typval::tv_dict_unref(msgpack_types);
            set_vim_var_dict(VimVarIndex::MsgpackTypes, std::ptr::null_mut());
            *EVAL_MSGPACK_TYPE_LISTS.get_mut() = [std::ptr::null_mut(); NUM_MSGPACK_TYPES];
            crate::eval::typval::tv_dict_unref(completed_item);
            set_vim_var_dict(VimVarIndex::CompletedItem, std::ptr::null_mut());
            crate::eval::typval::tv_dict_unref(event);
            set_vim_var_dict(VimVarIndex::Event, std::ptr::null_mut());
            crate::eval::typval::tv_list_unref(errors);
            set_vim_var_list(VimVarIndex::Errors, std::ptr::null_mut());
            crate::eval::typval::partial_unref(lua_partial);
            set_vim_var_partial(VimVarIndex::Lua, std::ptr::null_mut());
        }

        assert!(crate::eval::typval::gc_first_dict_is_empty());
        assert!(crate::eval::typval::gc_first_list_is_empty());
    }
}

/// Mark all lists/dicts referenced through the global (`g:`) scope
/// with `copy_id` (`garbage_collect_globvars`).
///
/// # Safety
/// Every item transitively reachable from `GLOBVARDICT` must be
/// valid, satisfying [`crate::eval::eval::set_ref_in_ht`]'s own safety
/// contract.
#[must_use]
pub unsafe fn garbage_collect_globvars(copy_id: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::eval::set_ref_in_ht(get_globvar_dict(), copy_id, std::ptr::null_mut()) }
}

/// Mark all lists/dicts referenced through every registered script's
/// own `s:` scope with `copy_id` (`garbage_collect_scriptvars`).
///
/// # Safety
/// Every item transitively reachable from every registered script's
/// own `s:` scope dict must be valid.
#[must_use]
pub unsafe fn garbage_collect_scriptvars(copy_id: i32) -> bool {
    let mut abort = false;
    for i in 1..=crate::runtime::script_item_count() {
        let item = crate::runtime::script_item(i);
        if item.is_null() {
            continue;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let sv = unsafe { (*item).sn_vars };
        if sv.is_null() {
            continue;
        }
        // SAFETY: forwarded from this function's own safety doc.
        abort = abort
            || unsafe {
                crate::eval::eval::set_ref_in_ht(
                    &mut (*sv).sv_dict as *mut DictT,
                    copy_id,
                    std::ptr::null_mut(),
                )
            };
    }
    abort
}

/// The `compat_hashtab` file-static - names valid in ALL scopes that
/// should also be found via implicit (no-scope-prefix) lookup, e.g.
/// `"version"` for `v:version` (`compat_hashtab`).
///
/// UNLIKE `GLOBVARDICT`/`FUNCARGS` (empty until a not-yet-translated
/// populator runs), this self-populates on first access: every
/// `VV_COMPAT`-flagged `VIMVARS` entry's own key is added here,
/// mirroring `evalvars_init`'s own `if (p->vv_flags & VV_COMPAT)
/// hash_add(&compat_hashtab, p->vv_di.di_key)` (`vars.c` line
/// 283-286). Deliberately independent of [`VIMVARDICT`]'s own
/// population (rather than one populating the other as a side
/// effect) - whichever of the two statics is touched first must see
/// fully-correct content regardless of access order, and each loop
/// only ever touches its OWN hashtable, so there is no double-`hash_add`
/// hazard either way.
static COMPAT_HASHTAB: std::sync::LazyLock<crate::globals::GlobalCell<crate::hashtab_defs::HashtabT>> =
    std::sync::LazyLock::new(|| {
        let mut ht = crate::hashtab_defs::HashtabT::hash_init();
        // SAFETY: only touches this module's own VIMVARS cell (via
        // vimvar_ptr_at, never creating a `&mut Vec`/`&mut [Vimvar]`
        // reference - see that function's own doc comment), and every
        // di_key pointer added here is owned by that same Vec's own
        // element (never resized/freed afterward - see get_vim_var_tv's
        // own doc comment), so it outlives this hashtable entry.
        for i in 0..vimvars_len() {
            let v = vimvar_ptr_at(i);
            if unsafe { (*v).flags } & vv_flag::COMPAT != 0 {
                let key_ptr = unsafe { (*v).di.di_key.as_mut_ptr() as *mut std::os::raw::c_char };
                unsafe { ht.hash_add(key_ptr) };
            }
        }
        crate::globals::GlobalCell::new(ht)
    });

/// The `v:` scope dict (`vimvardict`; `vimvarht` is just
/// `vimvardict.dv_hashtab` in the original, via a `#define`).
///
/// `dv_lock`/`dv_scope`/`dv_refcount` now match `evalvars_init`'s own
/// `init_var_dict(&vimvardict, &vimvars_var, VAR_SCOPE);
/// vimvardict.dv_lock = VAR_FIXED;` calls' real effect on the DICT
/// itself (`Fixed`/`Scope`/`DO_NOT_FREE_CNT`) - added alongside
/// `VIMVARS_VAR` below, since both represent the same slice of
/// `init_var_dict`'s work (matching `GLOBVARDICT`/`GLOBVARS_VAR`'s own
/// analogous completion).
///
/// UNLIKE `GLOBVARDICT` (which starts genuinely empty - real `g:`
/// variables only ever come from user `:let` commands, so there is no
/// fixed set of names to pre-populate), `v:` has a fixed,
/// compile-time-known set of ~108 names that must always be
/// resolvable through the dict-lookup path too - so this static DOES
/// perform the actual population loop from `evalvars_init` (`vars.c`
/// lines 269-282, the `vimvarht`-populating half only - see
/// `COMPAT_HASHTAB`'s own doc comment for why the `compat_hashtab`
/// half lives independently instead of as a shared side effect):
/// every `VIMVARS` entry whose value isn't `VAR_UNKNOWN` (i.e. every
/// entry except `v:val`/`v:key` - see `VIMVARS`'s own doc comment) is
/// added to this dict's own `dv_hashtab`/`dv_index`, pointing directly
/// at that entry's own embedded `di` (`DictitemT`) - exactly mirroring
/// the original's `vimvars[i].vv_di` being embedded storage, addressed
/// directly by `vimvarht`'s hash items, with no separate allocation or
/// synchronization ever needed. This is safe because `VIMVARS`'s
/// backing `Vec` is populated once and never resized afterward (see
/// `get_vim_var_tv`'s own doc comment) - `&mut v.di` stays a valid,
/// stable address for the rest of the program, exactly like a real
/// static array element's address in the original.
///
/// `evalvars_init` ITSELF is now fully translated (see its own doc
/// comment) and sets most `VIMVARS` entries' real runtime VALUES -
/// only `v:version`/`v:versionlong` (needs `version.c`'s generated
/// version-history tables) and `v:msgpack_types` (needs the JSON/
/// msgpack encoding subsystem) remain genuinely deferred. This static
/// only ever builds the DICT STRUCTURE regardless (matching this
/// crate's usual "structure before the engine/values" precedent, e.g.
/// `OptIndex`/`CmdIdxT` before their real populated tables) - it does
/// not itself call `evalvars_init`, since a `LazyLock`'s own init
/// closure has no way to also run an unrelated, separately-invoked
/// bootstrap function.
static VIMVARDICT: std::sync::LazyLock<crate::globals::GlobalCell<DictT>> =
    std::sync::LazyLock::new(|| {
        let mut dict = DictT {
            dv_lock: VarLockStatus::Fixed,
            dv_scope: ScopeType::Scope,
            dv_refcount: DO_NOT_FREE_CNT,
            dv_copy_id: 0,
            dv_hashtab: crate::hashtab_defs::HashtabT::hash_init(),
            dv_index: std::collections::HashMap::new(),
            dv_copydict: std::ptr::null_mut(),
            dv_used_next: std::ptr::null_mut(),
            dv_used_prev: std::ptr::null_mut(),
            lua_table_ref: LUA_NOREF,
        };
        // SAFETY: only touches this module's own VIMVARS cell, via
        // vimvar_ptr_at - never creating a `&mut Vec`/`&mut [Vimvar]`
        // reference (see that function's own doc comment) - so the
        // pointers stored into dv_index below remain valid across
        // every LATER, independent call to any
        // get_vim_var_*/set_vim_var_* function (which also now uses
        // vimvar_ptr/vimvar_ptr_at exclusively, for the same reason).
        // Proven necessary for real by Miri's Tree Borrows checker: the
        // original `VIMVARS.get_mut().iter_mut()`-derived pointers here
        // were invalidated by set_vim_var_dict's own (then-separate)
        // `VIMVARS.get_mut()[idx]` call - a genuine bug, not just a
        // theoretical one.
        for i in 0..vimvars_len() {
            let v = vimvar_ptr_at(i);
            if unsafe { (*v).di.di_tv.value.var_type() } == VarType::Unknown {
                continue;
            }
            let key_ptr = unsafe { (*v).di.di_key.as_mut_ptr() as *mut std::os::raw::c_char };
            unsafe { dict.dv_hashtab.hash_add(key_ptr) };
            dict.dv_index.insert(key_ptr as usize, unsafe { std::ptr::addr_of_mut!((*v).di) });
        }
        crate::globals::GlobalCell::new(dict)
    });

/// @return the `v:` variable dictionary (`get_vimvar_dict`).
///
/// Uses `VIMVARDICT.as_ptr()` (not `.get_mut()`), for the exact same
/// reason as [`get_globvar_dict`] - see that function's own doc
/// comment.
#[must_use]
pub fn get_vimvar_dict() -> *mut DictT {
    // SAFETY: VIMVARDICT is only ever read/written through this
    // module's own functions.
    VIMVARDICT.as_ptr()
}

/// The `vimvars_var` file-static - the whole `v:` scope, "as if it
/// were one `dictitem_T`" (really a [`ScopeDictDictItem`], per
/// [`DictitemVariant`]'s own doc comment). Only ever consumed via
/// [`find_var_in_ht`]'s own `varname_len == 0` (implicit whole-scope)
/// branch, matching the original's sole real use
/// (`(dictitem_T *)&vimvars_var`). Kept private, matching the
/// original's own file-static visibility (`vimvars_var` is never
/// referenced outside `vars.c` either).
static VIMVARS_VAR: std::sync::LazyLock<crate::globals::GlobalCell<ScopeDictDictItem>> =
    std::sync::LazyLock::new(|| {
        crate::globals::GlobalCell::new(ScopeDictDictItem {
            di_tv: TypvalT { v_lock: VarLockStatus::Fixed, value: TypvalValue::Dict(get_vimvar_dict()) },
            di_flags: dict_item_flags::RO | dict_item_flags::FIX,
            di_key: vec![0], // empty NUL-terminated key, matching di_key[0] = NUL
        })
    });

/// Every item transitively reachable from `get_vimvar_dict()` (the
/// `v:` scope) is kept alive by marking it with `copy_id`
/// (`garbage_collect_vimvars`).
///
/// # Safety
/// Same as [`crate::eval::eval::set_ref_in_ht`].
pub unsafe fn garbage_collect_vimvars(copy_id: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::eval::set_ref_in_ht(get_vimvar_dict(), copy_id, std::ptr::null_mut()) }
}

/// Find the hashtable and owning dict used for a variable
/// (`find_var_ht_dict`).
///
/// Collapses the original's `const char **varname`/`dict_T **d`
/// out-parameters into part of the return value: `(hashtab,
/// clean_name, dict)`. `hashtab`/`dict` are null if `name` doesn't
/// resolve to a known scope. `clean_name` is `name`'s own suffix with
/// any scope prefix (`g:`/`b:`/etc.) stripped.
///
/// The original's `nlua_set_sctx(&current_sctx)` call inside the `s:`
/// branch is omitted - it only resolves a Lua filename/line number for
/// "last set" diagnostic messages, never affecting
/// `current_sctx.sc_sid` itself (confirmed by reading its own real
/// body), so this omission doesn't change which dict/hashtable is
/// ultimately resolved.
#[must_use]
pub fn find_var_ht_dict(name: &[u8]) -> (*mut HashtabT, &[u8], *mut DictT) {
    if name.is_empty() {
        return (std::ptr::null_mut(), name, std::ptr::null_mut());
    }

    let mut d: *mut DictT = std::ptr::null_mut();
    let varname: &[u8];

    if name.len() == 1 || name.get(1) != Some(&b':') {
        // name has implicit scope
        if name[0] == b':' || name[0] == crate::eval::eval::AUTOLOAD_CHAR {
            // The name must not start with a colon or #.
            return (std::ptr::null_mut(), name, std::ptr::null_mut());
        }
        varname = name;

        // "version" is "v:version" in all scopes.
        // SAFETY: only touches this module's own COMPAT_HASHTAB cell.
        let found = !hashitem_empty(unsafe { COMPAT_HASHTAB.get_mut() }.hash_find(name));
        if found {
            // SAFETY: forwarded from the same reasoning above.
            return (
                unsafe { COMPAT_HASHTAB.get_mut() as *mut crate::hashtab_defs::HashtabT },
                varname,
                std::ptr::null_mut(),
            );
        }

        d = get_funccal_local_dict();
        if d.is_null() {
            d = get_globvar_dict(); // global variable
        }
    } else {
        varname = &name[2..];
        if name[0] == b'g' {
            // global variable
            d = get_globvar_dict();
        } else if name.len() > 2
            && (name[2..].contains(&b':') || name[2..].contains(&crate::eval::eval::AUTOLOAD_CHAR))
        {
            // There must be no ':' or '#' in the rest of the name if
            // g: was not used.
            return (std::ptr::null_mut(), varname, std::ptr::null_mut());
        }

        // SAFETY: curbuf/curwin/curtab are always valid pointers to
        // the real current buffer/window/tabpage in a running crate
        // instance, matching the original's own unchecked dereference.
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        match name[0] {
            b'b' => d = unsafe { (*g.curbuf).b_vars },  // buffer variable
            b'w' => d = unsafe { (*g.curwin).w_vars },  // window variable
            b't' => d = unsafe { (*g.curtab).tp_vars }, // tab page variable
            b'v' => d = get_vimvar_dict(), // v: variable
            b'a' => d = get_funccal_args_dict(), // a: function argument
            b'l' => d = get_funccal_local_dict(), // l: local variable
            b's'
                if (g.current_sctx.sc_sid > 0
                    || g.current_sctx.sc_sid == crate::globals::SID_STR
                    || g.current_sctx.sc_sid == crate::globals::SID_LUA)
                    && g.current_sctx.sc_sid <= crate::runtime::script_item_count() =>
            {
                // script variable. For anonymous scripts without a
                // script item, create one now so script vars can be
                // used.
                if g.current_sctx.sc_sid == crate::globals::SID_STR
                    || g.current_sctx.sc_sid == crate::globals::SID_LUA
                {
                    // Create SID if s: scope is accessed from Lua or
                    // anon Vimscript.
                    let (new_sid, _) = crate::runtime::new_script_item(None);
                    g.current_sctx.sc_sid = new_sid;
                }
                let item = crate::runtime::script_item(g.current_sctx.sc_sid);
                // SAFETY: script_item never returns null for an id
                // within 1..=script_item_count(), which the guard
                // above (or new_script_item's own freshly-created sid)
                // ensures.
                d = unsafe { &mut (*(*item).sn_vars).sv_dict as *mut DictT };
            }
            _ => {}
        }
    }

    let ht = if d.is_null() {
        std::ptr::null_mut()
    } else {
        // SAFETY: d just checked non-null above.
        unsafe { &mut (*d).dv_hashtab as *mut HashtabT }
    };
    (ht, varname, d)
}

/// Find the hashtable used for a variable (`find_var_ht`).
///
/// Drops the original's `dict_T **d` out-parameter entirely - unlike
/// [`find_var_ht_dict`], `find_var_ht` itself never uses it (the
/// original computes it into a throwaway local and discards it too).
///
/// @return the scope hashtable (null if `name` is not valid) and the
/// clean name without its scope prefix.
#[must_use]
pub fn find_var_ht(name: &[u8]) -> (*mut HashtabT, &[u8]) {
    let (ht, varname, _d) = find_var_ht_dict(name);
    (ht, varname)
}

/// Find variable in hashtab. When `varname` is empty, returns the
/// scope's own pseudo-item (as if the WHOLE scope were one
/// `dictitem_T`) instead (`find_var_in_ht`).
///
/// Takes `d: *mut DictT` (the owning dict) rather than the original's
/// bare `hashtab_T *ht` - see `vars_clear_ext`'s own doc comment for
/// why every item-recovery function in this crate needs the owning
/// `DictT`, not just its bare hashtable (`dv_index` substitutes for
/// the original's `TV_DICT_HI2DI` pointer-arithmetic recovery).
/// `htname` is the scope letter (`b's'`/`b'g'`/`b'v'`/`b'b'`/`b'w'`/
/// `b't'`/`b'l'`/`b'a'`), matching the original's `int htname`
/// parameter (every real caller passes `*name`, the whole name's
/// first byte, before any scope-prefix stripping - only meaningful
/// when `varname` is empty, i.e. `name` was something like `"s:"`).
/// Returns [`DictitemVariant`] rather than a bare `dictitem_T*` - see
/// that type's own doc comment for why.
///
/// The original's `ht == get_globvar_ht()` autoload-eligibility check
/// becomes `d == get_globvar_dict()` here - equivalent (a hashtable is
/// always embedded 1:1 in its owning dict) and simpler, since `d` is
/// already this function's own parameter with no reborrow needed;
/// [`get_globvar_ht`] itself is still translated as a real,
/// independent accessor for other future callers (e.g. `shada.c`,
/// which uses it directly, not through this function).
///
/// @return the found item, or `None` if `varname` doesn't exist in
/// `d`'s hashtable (and, for the global scope specifically, still
/// doesn't exist after attempting to autoload it).
///
/// # Safety
/// `d`, if `varname` is non-empty, must be a valid, non-null pointer
/// to a live `DictT` (matching `FUNC_ATTR_NONNULL_ALL` on the
/// original - the original never null-checks `ht` itself either).
/// `GLOBALS.curbuf`/`curwin`/`curtab` must be valid (only actually
/// dereferenced when `htname` is `b'b'`/`b'w'`/`b't'` AND `varname` is
/// empty). If `htname == b's'`, `GLOBALS.current_sctx.sc_sid` must be
/// a valid, already-created script ID - guaranteed by every real
/// caller, which always derives `d`/`htname` from a preceding
/// `find_var_ht_dict` call whose own `'s'` branch already validated
/// or lazily created it.
#[must_use]
pub unsafe fn find_var_in_ht(
    d: *mut DictT,
    htname: u8,
    varname: &[u8],
    no_autoload: bool,
) -> Option<DictitemVariant> {
    if varname.is_empty() {
        // Must be something like "s:", otherwise "d" would be null.
        return match htname {
            b's' => {
                // SAFETY: forwarded from this function's own safety doc.
                let sid = unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx.sc_sid;
                let item = crate::runtime::script_item(sid);
                // SAFETY: script_item never returns null for an
                // already-valid sid, guaranteed by this function's
                // own safety doc.
                let sv = unsafe { (*item).sn_vars };
                // SAFETY: sv is a live ScriptvarT, guaranteed by
                // script_item's own contract once item is non-null.
                Some(DictitemVariant::Scope(unsafe { &mut (*sv).sv_var }))
            }
            b'g' => Some(DictitemVariant::Scope(
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { GLOBVARS_VAR.get_mut() as *mut ScopeDictDictItem },
            )),
            b'v' => Some(DictitemVariant::Scope(
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { VIMVARS_VAR.get_mut() as *mut ScopeDictDictItem },
            )),
            b'b' => {
                // SAFETY: forwarded from this function's own safety doc.
                let g = unsafe { crate::globals::GLOBALS.get_mut() };
                Some(DictitemVariant::Scope(unsafe { &mut (*g.curbuf).b_bufvar }))
            }
            b'w' => {
                // SAFETY: forwarded from this function's own safety doc.
                let g = unsafe { crate::globals::GLOBALS.get_mut() };
                Some(DictitemVariant::Scope(unsafe { &mut (*g.curwin).w_winvar }))
            }
            b't' => {
                // SAFETY: forwarded from this function's own safety doc.
                let g = unsafe { crate::globals::GLOBALS.get_mut() };
                Some(DictitemVariant::Scope(unsafe { &mut (*g.curtab).tp_winvar }))
            }
            b'l' => {
                let v = crate::eval::userfunc::get_funccal_local_var();
                if v.is_null() { None } else { Some(DictitemVariant::Scope(v)) }
            }
            b'a' => {
                let v = crate::eval::userfunc::get_funccal_args_var();
                if v.is_null() { None } else { Some(DictitemVariant::Scope(v)) }
            }
            _ => None,
        };
    }

    // SAFETY: forwarded from this function's own safety doc.
    let dict = unsafe { &mut *d };
    let mut hi = dict.dv_hashtab.hash_find(varname);
    if hashitem_empty(hi) {
        // For global variables we may try auto-loading the script. If
        // it worked find the variable again. Don't auto-load a script
        // if it was loaded already, otherwise it would be loaded every
        // time when checking if a function name is a Funcref variable.
        if d == get_globvar_dict() && !no_autoload {
            // Note: script_autoload() may make "hi" invalid. It must
            // either be obtained again or not used - re-fetched below
            // regardless.
            if !crate::runtime::script_autoload(varname, false) || crate::ex_eval::aborting() {
                return None;
            }
            hi = dict.dv_hashtab.hash_find(varname);
        }
        if hashitem_empty(hi) {
            return None;
        }
    }
    let hi_key = hi.hi_key as usize;
    dict.dv_index.get(&hi_key).copied().map(DictitemVariant::Dict)
}

/// Find variable `name` in the list of variables (`find_var`).
///
/// Careful: `a:0`-style variables don't have a real name.
///
/// Collapses the original's `hashtab_T **htp` out-parameter into part
/// of the return value (`(item, ht)`), matching `find_var_ht_dict`'s
/// own established precedent - `want_ht` replaces the original's
/// `htp != NULL` test, which (besides controlling whether `*htp` gets
/// written at all) ALSO faithfully forces `no_autoload` on, exactly
/// matching the original's own `no_autoload || htp != NULL` at both
/// call sites below.
///
/// @return `(item, ht)` - `item` is `None` if `name` doesn't resolve
/// to any variable; `ht` is the scope hashtable `name`'s OWN prefix
/// resolved to (null if `name` itself is invalid) - notably, if
/// `item` was actually found via the parent (lambda-enclosing) scope
/// search, `ht` STILL reflects the ORIGINAL (inner) scope, never the
/// parent's - a faithfully-preserved quirk of the original, which
/// only ever captures `htp` once, before descending into
/// `find_var_in_scoped_ht`.
///
/// # Safety
/// Same as [`find_var_in_ht`]/
/// [`crate::eval::userfunc::find_var_in_scoped_ht`].
#[must_use]
pub unsafe fn find_var(
    name: &[u8],
    want_ht: bool,
    no_autoload: bool,
) -> (Option<DictitemVariant>, *mut HashtabT) {
    let (ht, varname, d) = find_var_ht_dict(name);
    if ht.is_null() {
        return (None, ht);
    }
    let no_autoload = no_autoload || want_ht;
    // SAFETY: forwarded from this function's own safety doc. ht
    // non-null (just checked above) guarantees name is non-empty, so
    // name[0] cannot panic.
    let ret = unsafe { find_var_in_ht(d, name[0], varname, no_autoload) };
    if ret.is_some() {
        return (ret, ht);
    }

    // Search in parent scope for lambda.
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { crate::eval::userfunc::find_var_in_scoped_ht(name, no_autoload) }, ht)
}

/// Get the value of variable `name`, copying it into `rettv`
/// (`eval_variable`).
///
/// Returns `crate::vim_defs::FAIL` when `name` doesn't resolve to any
/// variable, `crate::vim_defs::OK` otherwise (matching the original's
/// own `int` return, not a `bool`, since this crate's established
/// convention reserves `bool` for functions with no historical `OK`/
/// `FAIL`-constant call sites - `eval7`, this function's real caller,
/// checks the result the same way as every other `eval*` function).
///
/// Drops the original's `dip: dictitem_T **` out-parameter (letting a
/// caller later WRITE BACK into the found item, e.g. for `:let`
/// augmented assignment) - no translated caller needs it yet
/// (`eval7`'s own call site always passes `NULL`); add it back the
/// same way [`find_var`] models `htp` if/when `get_lval` needs it.
///
/// # Safety
/// Forwarded from [`find_var`]'s own safety doc; if `rettv` is
/// `Some`, its own value must satisfy [`crate::eval::typval::tv_copy`]'s
/// safety contract as the "to" side (any old contents are overwritten
/// without being released first, matching `tv_copy`'s own documented
/// caller responsibility).
pub unsafe fn eval_variable(name: &[u8], rettv: Option<&mut TypvalT>, _verbose: bool, no_autoload: bool) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let (item, _ht) = unsafe { find_var(name, false, no_autoload) };

    match item {
        None => {
            // semsg(_("E121: Undefined variable: %.*s"), len, name)
            // omitted when rettv.is_some() && verbose - message
            // display, not tractable; the identical FAIL is kept.
            crate::vim_defs::FAIL
        }
        Some(variant) => {
            if let Some(rettv) = rettv {
                let src: *const TypvalT = match variant {
                    DictitemVariant::Dict(p) => {
                        // SAFETY: forwarded from this function's own safety doc.
                        unsafe { &(*p).di_tv }
                    }
                    DictitemVariant::Scope(p) => {
                        // SAFETY: forwarded from this function's own safety doc.
                        unsafe { &(*p).di_tv }
                    }
                };
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::eval::typval::tv_copy(&*src, rettv) };
            }
            crate::vim_defs::OK
        }
    }
}

/// Check if variable `var` exists (`var_exists`).
///
/// Only the common "plain name, no subscript" case is modeled -
/// mirrors the original's own call to `get_name_len(&var, &tofree,
/// true, false)` with `evaluate = true` exactly (so magic-brace name
/// expansion, e.g. `exists('foo{expr}bar')`, correctly panics via
/// [`crate::eval::eval::get_name_len`]'s own `unimplemented!()`,
/// rather than silently giving a wrong answer), followed by
/// [`crate::eval::eval::handle_subscript`] for any `.`/`[`/`(`/`->`
/// continuation (which similarly panics for anything beyond "nothing
/// follows" - see its own doc comment). Both panics match this
/// crate's established "translate the common path faithfully, panic
/// loudly on a genuinely untranslated-but-reached path" convention.
///
/// # Safety
/// Forwarded from [`eval_variable`]'s own safety doc.
#[must_use]
pub unsafe fn var_exists(var: &[u8]) -> bool {
    let (name_len, consumed) = crate::eval::eval::get_name_len(var, true);
    let mut n = false;
    let mut rest = &var[consumed.min(var.len())..];

    if name_len > 0 {
        let name = &var[..name_len];
        let mut tv = TypvalT::default();
        // SAFETY: forwarded from this function's own safety doc.
        n = unsafe { eval_variable(name, Some(&mut tv), false, true) } == crate::vim_defs::OK;
        if n {
            let mut evalarg = crate::eval::eval::EvalargT { eval_flags: crate::eval::eval::EVAL_EVALUATE, ..Default::default() };
            let (status, sub_consumed) =
                crate::eval::eval::handle_subscript(rest, &mut tv, Some(&mut evalarg), false);
            n = status == crate::vim_defs::OK;
            if n {
                // SAFETY: tv was just filled in by eval_variable above,
                // a fresh copy not shared with anything else yet.
                unsafe { crate::eval::typval::tv_clear_simple(&tv) };
            }
            rest = &rest[sub_consumed.min(rest.len())..];
        }
    }

    if !rest.is_empty() {
        n = false;
    }

    n
}

/// Convert an option value to a Vimscript value (`optval_as_tv`).
///
/// `numbool`, if `true`, converts a `Boolean` value to a plain number
/// (`0`/`1`/`-1` for `false`/`true`/unset) rather than a real
/// `Bool`/`Special(Null)` - matching [`crate::eval::eval::eval_option`]'s
/// own `numbool = true` call (options have always been numbers in
/// Vimscript, even boolean ones, for backward compatibility).
#[must_use]
pub fn optval_as_tv(value: crate::option_defs::OptVal, numbool: bool) -> TypvalT {
    use crate::option_defs::OptVal;
    use crate::types_defs::TriState;

    let value = match value {
        OptVal::Nil => TypvalValue::Special(SpecialVarValue::Null),
        OptVal::Boolean(b) => {
            if numbool {
                TypvalValue::Number(VarnumberT::from(b as i8))
            } else if b != TriState::None {
                TypvalValue::Bool(if b == TriState::True { BoolVarValue::True } else { BoolVarValue::False })
            } else {
                // return v:null for a None boolean value.
                TypvalValue::Special(SpecialVarValue::Null)
            }
        }
        OptVal::Number(n) => TypvalValue::Number(n),
        OptVal::String(s) => TypvalValue::String(Some(s)),
    };
    TypvalT { v_lock: VarLockStatus::Unlocked, value }
}

/// Evaluate a single embedded `{expr}` inside an interpolated string
/// and append its stringified result to `gap` (`eval_one_expr_in_str`).
///
/// `p` must point to the `{` itself. Returns the number of bytes of
/// `p` consumed (one past the matching `}`) on success, matching this
/// crate's own "return consumed bytes from start" idiom (the
/// original's own returned pointer, offset from `p`'s own start);
/// `None` on any error (an empty `{}`, an invalid expression, a
/// missing `}`, or - only when `evaluate` - a failed stringification).
///
/// Bounds the expression evaluator to exactly `[block_start,
/// block_end)` by slicing, rather than the original's own "temporarily
/// overwrite the `}` with a `NUL`, evaluate, then restore it" trick -
/// this crate's own byte-slice idiom already prevents the evaluator
/// from ever seeing anything past the intended end, with no mutation
/// (or restoration) of the input needed.
///
/// # Safety
/// Forwarded from [`crate::eval::eval::skip_expr`]/
/// [`crate::eval::eval::eval_to_string`]'s own safety docs.
#[must_use]
pub unsafe fn eval_one_expr_in_str(p: &[u8], gap: &mut Vec<u8>, evaluate: bool) -> Option<usize> {
    let block_start = 1 + crate::charset::skipwhite(&p[1..]); // skip the opening '{'

    // semsg(_(e_missing_close_curly_str), p) omitted on this and every
    // other error path below - message display, not tractable; the
    // identical None/FAIL is kept.
    p.get(block_start)?;

    // SAFETY: forwarded from this function's own safety doc.
    let (res, consumed) = unsafe { crate::eval::eval::skip_expr(&p[block_start..], None) };
    if res == crate::vim_defs::FAIL {
        return None;
    }
    let expr_end = block_start + consumed;
    let block_end = expr_end + crate::charset::skipwhite(&p[expr_end..]);

    if p.get(block_end) != Some(&b'}') {
        return None;
    }

    if evaluate {
        // SAFETY: forwarded from this function's own safety doc.
        let expr_val = unsafe { crate::eval::eval::eval_to_string(&p[block_start..block_end], false, false) }?;
        gap.extend_from_slice(&expr_val);
    }

    Some(block_end + 1)
}

/// Out-flag for the not-yet-translated lambda-compilation code: when
/// `Some`, [`check_vars`] sets the pointed-to `bool` to `true` upon
/// finding that a checked name resolves to a local variable or
/// argument (`eval_lavars_used`).
///
/// Always `None` today - nothing in this crate constructs a real
/// lambda body needing this tracked yet (needs `get_lambda_tv`, not
/// yet translated), matching the original's own `NULL` default before
/// any lambda compilation begins.
static EVAL_LAVARS_USED: crate::globals::GlobalCell<Option<*mut bool>> = crate::globals::GlobalCell::new(None);

/// Check if variable `name` is a local variable or an argument - if
/// so, sets the flag `EVAL_LAVARS_USED` points to, if any
/// (`check_vars`).
///
/// A real, faithful (if currently always-inert) translation: kept as
/// its own function rather than omitted, ready for whenever lambda-
/// compilation code populates `EVAL_LAVARS_USED` for real.
///
/// # Safety
/// Forwarded from [`find_var`]'s own safety doc.
pub unsafe fn check_vars(name: &[u8]) {
    // SAFETY: EVAL_LAVARS_USED is a private, crate-internal GlobalCell
    // only ever touched by this function and (in the future) lambda-
    // compilation code.
    let Some(flag) = (unsafe { *EVAL_LAVARS_USED.get_mut() }) else {
        return;
    };

    let (ht, _varname) = find_var_ht(name);
    if ht == crate::eval::userfunc::get_funccal_local_ht() || ht == crate::eval::userfunc::get_funccal_args_ht()
    {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { find_var(name, false, true) }.0.is_some() {
            // SAFETY: forwarded from this function's own safety doc -
            // a non-null EVAL_LAVARS_USED must point at a valid,
            // live bool, exactly like the original's own contract for
            // eval_lavars_used.
            unsafe { *flag = true };
        }
    }
}

/// Get a raw pointer to `VIMVARS`'s element at raw index `idx`,
/// without ever creating a `&mut Vec<Vimvar>`/`&mut [Vimvar]`
/// reference the way `VIMVARS.get_mut()[idx]` indexing does - see
/// `GlobalCell::as_ptr`'s own doc comment for why this matters:
/// [`VIMVARDICT`]'s own `dv_index` holds long-lived pointers into
/// these same elements (derived the same way, via this helper), which
/// must remain valid across every later call to any
/// `get_vim_var_*`/`set_vim_var_*` function - proven for real by
/// Miri's Tree Borrows checker to otherwise be a genuine bug (a
/// `.get_mut()[idx]`-derived pointer stored in `VIMVARDICT.dv_index`
/// was invalidated by a LATER, unrelated `.get_mut()[idx]` call from
/// `set_vim_var_dict`).
///
/// Takes a raw `usize` (not [`VimVarIndex`]) so [`COMPAT_HASHTAB`]'s/
/// [`VIMVARDICT`]'s own construction can iterate every entry (`0..108`)
/// without needing a `usize -> VimVarIndex` conversion - [`vimvar_ptr`]
/// (below) is the type-safe wrapper real accessor functions use.
///
/// # Safety
/// `idx` must be in `VIMVARS`'s own fixed 108-entry range.
fn vimvar_ptr_at(idx: usize) -> *mut Vimvar {
    // SAFETY: VIMVARS's own LazyLock ensures it is populated (a fixed
    // 108 entries, never resized afterward - see this function's own
    // safety doc) before this pointer is ever dereferenced elsewhere.
    // as_ptr() never creates a reference, so calling this repeatedly -
    // even while other pointers derived the same way remain live - is
    // sound, unlike get_mut().
    let vimvars_ptr = VIMVARS.as_ptr();
    // SAFETY: forwarded from this function's own safety doc - the
    // `&mut Vec` this briefly, implicitly creates (to call
    // `as_mut_ptr`) is used and discarded immediately, never held
    // across any other access.
    let buf_ptr = unsafe { (*vimvars_ptr).as_mut_ptr() };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { buf_ptr.add(idx) }
}

/// Get a raw pointer to `VIMVARS`'s element at `idx` - the type-safe,
/// [`VimVarIndex`]-taking wrapper around [`vimvar_ptr_at`] every real
/// `get_vim_var_*`/`set_vim_var_*` accessor uses. See that function's
/// own doc comment for the full reasoning.
///
/// # Safety
/// `idx` must be a valid `VimVarIndex` variant (always true - every
/// variant is in `VIMVARS`'s own fixed 108-entry range by
/// construction).
fn vimvar_ptr(idx: VimVarIndex) -> *mut Vimvar {
    vimvar_ptr_at(idx as usize)
}

/// The number of entries in `VIMVARS` (always 108, its own fixed,
/// populate-once length) - used by [`COMPAT_HASHTAB`]'s/
/// [`VIMVARDICT`]'s own construction to iterate every raw index via
/// [`vimvar_ptr_at`], matching that helper's own "never create a
/// `&mut Vec`/`&mut [Vimvar]` reference" reasoning.
fn vimvars_len() -> usize {
    // SAFETY: forwarded from vimvar_ptr_at's own safety doc - the
    // `&mut Vec` this briefly, implicitly creates (to call `.len()`)
    // is used and discarded immediately.
    unsafe { (*VIMVARS.as_ptr()).len() }
}

/// Get the name of `v:` variable `idx`, without the `v:` prefix
/// (`get_vim_var_name`).
#[must_use]
pub fn get_vim_var_name(idx: VimVarIndex) -> &'static str {
    // SAFETY: forwarded from vimvar_ptr's own safety doc.
    unsafe { (*vimvar_ptr(idx)).name }
}

/// Get a raw pointer to `v:` variable `idx`'s own `typval_T`
/// (`get_vim_var_tv`).
///
/// # Safety
/// The returned pointer stays valid as long as `VIMVARS` itself (the
/// whole program's lifetime, in practice): its backing `Vec` is
/// populated once, with a fixed 108 entries, and never resized
/// afterward by any function in this module, so indexing into it can
/// never be invalidated by reallocation. Derived via `vimvar_ptr`
/// (not `VIMVARS.get_mut()[idx]` directly) so the returned pointer
/// safely remains valid across any LATER call to another
/// `get_vim_var_*`/`set_vim_var_*` function too - see that helper's
/// own doc comment.
#[must_use]
pub unsafe fn get_vim_var_tv(idx: VimVarIndex) -> *mut TypvalT {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { std::ptr::addr_of_mut!((*vimvar_ptr(idx)).di.di_tv) }
}

/// Get number `v:` variable `idx`'s value (`get_vim_var_nr`).
///
/// # Panics
/// If `idx`'s value isn't [`TypvalValue::Number`] - the original does
/// a raw, unchecked union read here (`tv->vval.v_number`) with no type
/// check at all; every real caller only ever calls this on an
/// already-Number-typed slot (see this module's own doc comment), so
/// this is a faithful "must only be called on a Number-typed slot"
/// caller contract, not a narrowing.
///
/// # Safety
/// Same as [`get_vim_var_tv`].
#[must_use]
pub unsafe fn get_vim_var_nr(idx: VimVarIndex) -> VarnumberT {
    // SAFETY: forwarded from this function's own safety doc.
    match unsafe { &*get_vim_var_tv(idx) }.value {
        TypvalValue::Number(n) => n,
        ref other => panic!(
            "get_vim_var_nr: v:{} is not Number-typed (found {other:?})",
            get_vim_var_name(idx)
        ),
    }
}

/// Get List `v:` variable `idx`'s value. Caller must take care of the
/// reference count when needed (`get_vim_var_list`).
///
/// # Panics
/// Same contract as [`get_vim_var_nr`], for [`TypvalValue::List`].
///
/// # Safety
/// Same as [`get_vim_var_tv`].
#[must_use]
pub unsafe fn get_vim_var_list(idx: VimVarIndex) -> *mut crate::eval::typval_defs::ListT {
    // SAFETY: forwarded from this function's own safety doc.
    match unsafe { &*get_vim_var_tv(idx) }.value {
        TypvalValue::List(l) => l,
        ref other => panic!(
            "get_vim_var_list: v:{} is not List-typed (found {other:?})",
            get_vim_var_name(idx)
        ),
    }
}

/// Get Dictionary `v:` variable `idx`'s value. Caller must take care
/// of the reference count when needed (`get_vim_var_dict`).
///
/// # Panics
/// Same contract as [`get_vim_var_nr`], for [`TypvalValue::Dict`].
///
/// # Safety
/// Same as [`get_vim_var_tv`].
#[must_use]
pub unsafe fn get_vim_var_dict(idx: VimVarIndex) -> *mut DictT {
    // SAFETY: forwarded from this function's own safety doc.
    match unsafe { &*get_vim_var_tv(idx) }.value {
        TypvalValue::Dict(d) => d,
        ref other => panic!(
            "get_vim_var_dict: v:{} is not Dict-typed (found {other:?})",
            get_vim_var_name(idx)
        ),
    }
}

/// Get string `v:` variable `idx`'s value. If the string variable has
/// never been set, returns an empty string (`get_vim_var_str`).
///
/// Unlike [`get_vim_var_nr`]/[`get_vim_var_list`]/[`get_vim_var_dict`],
/// this can never panic: the original's own `tv_get_string` already
/// gracefully stringifies every possible `v_type` (numbers, floats,
/// bools, etc.), matching [`crate::eval::typval::tv_get_string`]
/// exactly - no caller-contract issue to preserve here.
///
/// # Safety
/// Same as [`get_vim_var_tv`].
#[must_use]
pub unsafe fn get_vim_var_str(idx: VimVarIndex) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    crate::eval::typval::tv_get_string(unsafe { &*get_vim_var_tv(idx) })
}

/// Get Partial `v:` variable `idx`'s value. Caller must take care of
/// the reference count when needed (`get_vim_var_partial`).
///
/// # Panics
/// Same contract as [`get_vim_var_nr`], for [`TypvalValue::Partial`].
///
/// # Safety
/// Same as [`get_vim_var_tv`].
#[must_use]
pub unsafe fn get_vim_var_partial(idx: VimVarIndex) -> *mut crate::eval::typval_defs::PartialT {
    // SAFETY: forwarded from this function's own safety doc.
    match unsafe { &*get_vim_var_tv(idx) }.value {
        TypvalValue::Partial(p) => p,
        ref other => panic!(
            "get_vim_var_partial: v:{} is not Partial-typed (found {other:?})",
            get_vim_var_name(idx)
        ),
    }
}

/// Set `v:` variable `idx`'s value to a copy of `tv` (`set_vim_var_tv`).
///
/// # Safety
/// Same as [`get_vim_var_tv`]. If `tv`'s value is
/// `List`/`Dict`/`Blob`/`Partial`-typed with a non-null pointer, that
/// pointer must be valid.
pub unsafe fn set_vim_var_tv(idx: VimVarIndex, tv: TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *get_vim_var_tv(idx) = tv };
}

/// Set the type of `v:` variable `idx` to `ty`, WITHOUT changing its
/// value (`set_vim_var_type`).
///
/// # Panics
/// If `ty` isn't [`VarType::Number`] - see this module's own doc
/// comment for why every real caller only ever passes `VAR_NUMBER`
/// here (always immediately followed by [`set_vim_var_nr`]).
///
/// # Safety
/// Same as [`get_vim_var_tv`].
pub unsafe fn set_vim_var_type(idx: VimVarIndex, ty: VarType) {
    assert_eq!(
        ty,
        VarType::Number,
        "set_vim_var_type: only VarType::Number is ever used by any real caller"
    );
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*get_vim_var_tv(idx)).value = TypvalValue::Number(0) };
}

/// Set number `v:` variable `idx` to `val`. Does not change the type -
/// see [`set_vim_var_type`] for that (`set_vim_var_nr`).
///
/// # Safety
/// Same as [`get_vim_var_tv`].
pub unsafe fn set_vim_var_nr(idx: VimVarIndex, val: VarnumberT) {
    // SAFETY: forwarded from this function's own safety doc. Directly
    // overwriting to Number(val) both releases whatever the slot
    // previously held (Rust's own Drop, matching tv_clear's effect)
    // and sets the new value - faithful to every real caller, which
    // only ever targets an already-Number-typed slot (see this
    // module's own doc comment).
    unsafe { (*get_vim_var_tv(idx)).value = TypvalValue::Number(val) };
}

/// Set boolean `v:` variable `idx` to `val` (`set_vim_var_bool`).
///
/// # Safety
/// Same as [`get_vim_var_tv`].
pub unsafe fn set_vim_var_bool(idx: VimVarIndex, val: BoolVarValue) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*get_vim_var_tv(idx)).value = TypvalValue::Bool(val) };
}

/// Set special `v:` variable `idx` to `val` (`set_vim_var_special`).
///
/// # Safety
/// Same as [`get_vim_var_tv`].
pub unsafe fn set_vim_var_special(idx: VimVarIndex, val: SpecialVarValue) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*get_vim_var_tv(idx)).value = TypvalValue::Special(val) };
}

/// Set string `v:` variable `idx` to a copy of `val`
/// (`set_vim_var_string`).
///
/// `val: None` matches the original's own `val == NULL` case
/// (`tv->vval.v_string = NULL`).
///
/// # Safety
/// Same as [`get_vim_var_tv`].
pub unsafe fn set_vim_var_string(idx: VimVarIndex, val: Option<&[u8]>) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*get_vim_var_tv(idx)).value = TypvalValue::String(val.map(<[u8]>::to_vec)) };
}

/// Set list `v:` variable `idx` to `val`. Reference count will be
/// incremented (`set_vim_var_list`).
///
/// # Safety
/// Same as [`get_vim_var_tv`]. `val`, if non-null, must be a valid
/// pointer to a live [`crate::eval::typval_defs::ListT`].
pub unsafe fn set_vim_var_list(idx: VimVarIndex, val: *mut crate::eval::typval_defs::ListT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*get_vim_var_tv(idx)).value = TypvalValue::List(val) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::typval::tv_list_ref(val) };
}

/// Set Dictionary `v:` variable `idx` to `val`. Reference count will
/// be incremented. Also keys of the dictionary will be made read-only
/// (`set_vim_var_dict`).
///
/// # Safety
/// Same as [`get_vim_var_tv`]. `val`, if non-null, must be a valid
/// pointer to a live [`DictT`].
pub unsafe fn set_vim_var_dict(idx: VimVarIndex, val: *mut DictT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*get_vim_var_tv(idx)).value = TypvalValue::Dict(val) };
    if val.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        (*val).dv_refcount += 1;
        crate::eval::typval::tv_dict_set_keys_readonly(val);
    }
}

/// Set Partial `v:` variable `idx` to `val`. Does not change the type
/// - see [`set_vim_var_type`] for that (`set_vim_var_partial`).
///
/// # Safety
/// Same as [`get_vim_var_tv`]. `val`, if non-null, must be a valid
/// pointer to a live [`crate::eval::typval_defs::PartialT`].
pub unsafe fn set_vim_var_partial(idx: VimVarIndex, val: *mut crate::eval::typval_defs::PartialT) {
    // SAFETY: forwarded from this function's own safety doc. Faithful
    // for the same reason as set_vim_var_nr - every real caller only
    // ever targets VV_LUA, already Partial-typed (see this module's
    // own doc comment).
    unsafe { (*get_vim_var_tv(idx)).value = TypvalValue::Partial(val) };
}

/// Set `v:char` to character `c` (`set_vim_var_char`).
///
/// # Safety
/// Same as [`get_vim_var_tv`].
pub unsafe fn set_vim_var_char(c: i32) {
    let mut buf = [0u8; crate::mbyte_defs::MB_MAXCHAR + 1];
    let buflen = crate::mbyte::utf_char2bytes(c, &mut buf);
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        set_vim_var_string(VimVarIndex::Char, Some(&buf[..buflen as usize]));
    }
}

/// Set `v:register` if needed (`set_reg_var`).
///
/// # Safety
/// Same as [`get_vim_var_tv`].
pub unsafe fn set_reg_var(c: i32) {
    let regname: u8 = if c == 0 || c == i32::from(b' ') { b'"' } else { c as u8 };
    // Avoid free/alloc when the value is already right.
    // SAFETY: forwarded from this function's own safety doc.
    let tv = unsafe { &*get_vim_var_tv(VimVarIndex::Reg) };
    let already_right = matches!(&tv.value, TypvalValue::String(Some(s)) if s.first() == Some(&regname));
    if !already_right {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { set_vim_var_string(VimVarIndex::Reg, Some(&[regname])) };
    }
}

/// Save `v:` variable `idx`'s current value into `save_tv`, in
/// preparation for a temporary override - e.g. `v:val`/`v:key` while
/// `filter()`/`map()`/`sort()`'s comparator expression is running
/// (`prepare_vimvar`).
///
/// If `idx`'s value is normally [`VarType::Unknown`] (not registered in
/// the real `v:` scope dict's hashtable at all by default - true of
/// `v:val`/`v:key` specifically, see `VIMVARS`'s own doc comment),
/// this ALSO adds it to `VIMVARDICT`'s `dv_hashtab`/`dv_index` for the
/// duration of the override, exactly mirroring the original's own
/// `hash_add(&vimvarht, vimvars[idx].vv_di.di_key)` - without this,
/// `v:val`/`v:key` would stay invisible to a real Vimscript expression
/// evaluated while the caller believes they're "set" (`find_var_in_ht`'s
/// real, hash-based lookup would never find them).
///
/// The original's own `vimvars[idx].vv_str = NULL;  // don't free it
/// now` has no Rust equivalent to translate: that line exists only to
/// stop the *next* line's plain struct-copy-without-clearing from
/// double-freeing a cached string pointer the original keeps alongside
/// `vv_tv` - this crate's `Vimvar`/`TypvalT` has no such separate cache,
/// and `TypvalT` itself implements no `Drop` at all (this crate's
/// established manual-memory-management style), so a plain
/// [`std::mem::take`] (leaving [`TypvalT::default`]/[`TypvalValue::Unknown`]
/// behind) is both safe and a faithful "just a bitwise struct copy, no
/// double-free risk" translation - the caller (`filter_map_one`,
/// `sort()`'s comparator, etc.) always immediately overwrites the slot
/// with a real new value anyway, so this difference is never observable.
///
/// # Safety
/// Same as [`get_vim_var_tv`].
pub unsafe fn prepare_vimvar(idx: VimVarIndex, save_tv: &mut TypvalT) {
    let v = vimvar_ptr(idx);
    // SAFETY: forwarded from this function's own safety doc.
    *save_tv = unsafe { std::mem::take(&mut (*v).di.di_tv) };
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { (*v).di.di_tv.value.var_type() } != VarType::Unknown {
        return;
    }
    // SAFETY: `di_key` is never mutated/freed while VIMVARS lives (see
    // VIMVARDICT's own construction, which relies on the exact same
    // stability), and outlives this whole hash-table entry's lifetime.
    let key_ptr = unsafe { (*v).di.di_key.as_mut_ptr() as *mut std::os::raw::c_char };
    let dict = get_vimvar_dict();
    // SAFETY: forwarded from this function's own safety doc; `dict` is
    // VIMVARDICT's own stable storage (see get_vimvar_dict's doc).
    unsafe {
        (*dict).dv_hashtab.hash_add(key_ptr);
        (*dict).dv_index.insert(key_ptr as usize, std::ptr::addr_of_mut!((*v).di));
    }
}

/// Restore `v:` variable `idx` to typval `save_tv` (`restore_vimvar`).
///
/// Note that the `v:` variable must have been cleared already (matching
/// the original's own doc comment) - the caller is responsible for
/// releasing whatever value the slot held immediately before this call,
/// exactly as with [`prepare_vimvar`]'s own caller-immediately-
/// overwrites contract.
///
/// When no longer defined (the slot's restored value is
/// [`VarType::Unknown`] - true of `v:val`/`v:key`), removes the entry
/// from `VIMVARDICT`'s `dv_hashtab`/`dv_index`, mirroring
/// [`prepare_vimvar`]'s own temporary registration exactly. The
/// original's `internal_error("restore_vimvar()")` (reached only if the
/// entry mysteriously isn't found - a genuine "should never happen"
/// caller-contract violation, since [`prepare_vimvar`] always adds it
/// first) becomes a `debug_assert!`, matching this crate's established
/// policy for this exact class of internal-invariant check.
///
/// # Safety
/// Same as [`get_vim_var_tv`].
pub unsafe fn restore_vimvar(idx: VimVarIndex, save_tv: TypvalT) {
    let v = vimvar_ptr(idx);
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*v).di.di_tv = save_tv };
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { (*v).di.di_tv.value.var_type() } != VarType::Unknown {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let key: &[u8] = unsafe { &(*v).di.di_key };
    // Strip the trailing NUL di_key always carries - hash_find/
    // hash_remove take the bare key bytes (matching
    // tv_dict_item_remove's own established treatment of this exact
    // convention).
    let key = &key[..key.len().saturating_sub(1)];
    let key_ptr = key.as_ptr();
    let dict = get_vimvar_dict();
    // SAFETY: forwarded from this function's own safety doc; `dict` is
    // VIMVARDICT's own stable storage.
    unsafe {
        let found = !hashitem_empty((*dict).dv_hashtab.hash_find(key));
        debug_assert!(found, "restore_vimvar: v:{} not found in vimvarht", (*v).name);
        if found {
            (*dict).dv_hashtab.hash_remove(key);
            (*dict).dv_index.remove(&(key_ptr as usize));
        }
    }
}

/// Set `v:count`/`v:count1`, and (if `set_prevcount`) `v:prevcount`
/// from the current `v:count` (`set_vcount`).
///
/// # Safety
/// Same as [`get_vim_var_tv`].
pub unsafe fn set_vcount(count: VarnumberT, count1: VarnumberT, set_prevcount: bool) {
    if set_prevcount {
        // SAFETY: forwarded from this function's own safety doc.
        let prev = unsafe { get_vim_var_nr(VimVarIndex::Count) };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { set_vim_var_nr(VimVarIndex::Prevcount, prev) };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_vim_var_nr(VimVarIndex::Count, count) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_vim_var_nr(VimVarIndex::Count1, count1) };
}

/// Whether `varname` is a valid variable name: every character is
/// either a name character (`eval_isnamec1`, plus digits after the
/// first position, plus the autoload separator), matching the
/// original's own per-character scan (`valid_varname`).
///
/// The original's `semsg(_(e_illvar), varname)` on the first invalid
/// character is omitted (message display, not tractable yet) - the
/// boolean result itself is kept exactly.
#[must_use]
pub fn valid_varname(varname: &[u8]) -> bool {
    for (i, &b) in varname.iter().enumerate() {
        if !crate::eval::eval::eval_isnamec1(b as i32)
            && (i == 0 || !crate::ascii_defs::ascii_isdigit(b as i32))
            && b != crate::eval::eval::AUTOLOAD_CHAR
        {
            return false;
        }
    }
    true
}

/// Whether it's NOT OK to change a variable with the given
/// `DictitemT.di_flags`: `true` when read-only, or
/// read-only-in-the-sandbox while currently inside the sandbox
/// (`var_check_ro`).
///
/// Drops the original's `name`/`name_len` parameters entirely - they
/// only ever affect the omitted `semsg()` message text, never the
/// return value, matching `value_check_lock`/`tv_check_lock`'s own
/// established precedent (`eval/typval.rs`) for this exact pattern.
#[must_use]
pub fn var_check_ro(flags: u8) -> bool {
    // SAFETY: only reads GLOBALS.sandbox, matching this crate's usual
    // "internal GlobalCell access, exposed as a safe pub fn" pattern
    // (e.g. function_list_modified).
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    flags & dict_item_flags::RO != 0 || (flags & dict_item_flags::RO_SBX != 0 && g.sandbox != 0)
}

/// Whether a variable with the given `di_flags` is locked
/// (`DI_FLAGS_LOCK`) (`var_check_lock`). See [`var_check_ro`]'s own
/// doc comment for why `name`/`name_len` are dropped.
#[must_use]
pub fn var_check_lock(flags: u8) -> bool {
    flags & dict_item_flags::LOCK != 0
}

/// Whether a variable with the given `di_flags` is fixed
/// (`DI_FLAGS_FIX`, cannot be `:unlet`/`remove()`d) (`var_check_fixed`).
/// See [`var_check_ro`]'s own doc comment for why `name`/`name_len`
/// are dropped.
#[must_use]
pub fn var_check_fixed(flags: u8) -> bool {
    flags & dict_item_flags::FIX != 0
}

/// Now that `dict` needs to be freed if no one else is using it, go
/// back to normal reference counting and unref it (`unref_var_dict`).
///
/// # Safety
/// `dict` must be a valid, non-null pointer satisfying
/// [`crate::eval::typval::tv_dict_unref`]'s own safety contract
/// (matching the original's own unchecked dereference - every real
/// caller passes an always-allocated `b_vars`/`w_vars`/`tp_vars`).
pub unsafe fn unref_var_dict(dict: *mut DictT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*dict).dv_refcount -= DO_NOT_FREE_CNT - 1 };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::typval::tv_dict_unref(dict) };
}

/// Like [`vars_clear`], but only free each item's value if
/// `free_val` (`vars_clear_ext`).
///
/// Takes `&mut DictT` rather than the original's bare `&mut
/// hashtab_T`: every real caller only ever passes
/// `&owning_dict.dv_hashtab` (`buffer.c`'s `b_vars`, `window.c`'s
/// `w_vars`/`t_vars`, `eval/userfunc.c`'s `fc_l_vars`/`fc_l_avars`,
/// this file's own script-vars) - this crate's `DictT.dv_index` side
/// table (the substitute for the original's `TV_DICT_HI2DI` pointer-
/// arithmetic recovery, see `DictitemT`'s own doc comment) needs the
/// OWNING `DictT`, not just its bare hashtable, to look items back up.
///
/// # Safety
/// Every item in `d.dv_index` must be a valid, non-null pointer
/// freeable via a plain `Box::from_raw` when `DI_FLAGS_ALLOC` is set
/// (matching [`crate::eval::typval::tv_dict_item_free`]'s own
/// analogous contract), and its `di_tv` must be safe to clear via
/// `tv_clear_simple` when `free_val` is `true`.
pub unsafe fn vars_clear_ext(d: &mut DictT, free_val: bool) {
    // Unlike the original (locks dv_hashtab, walks it via
    // HASHTAB_ITER + TV_DICT_HI2DI), dv_index already gives a direct
    // list of every live item - no hashtab traversal/locking needed,
    // matching tv_dict_free_contents's own established precedent.
    let items: Vec<*mut DictitemT> = d.dv_index.values().copied().collect();
    for item in items {
        if free_val {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_clear_simple(&(*item).di_tv) };
        }
        // SAFETY: forwarded from this function's own safety doc.
        let flags = unsafe { (*item).di_flags };
        if flags & dict_item_flags::ALLOC != 0 {
            if !free_val {
                // free_val=false means the value must be left
                // completely untouched here - some other code has
                // already taken over its ownership (e.g. moved a
                // List/Dict/Blob/Partial pointer elsewhere without
                // releasing this reference). Box::from_raw's own
                // implicit drop below would otherwise ALSO drop
                // di_tv automatically (Rust's normal field-drop,
                // unlike the original's plain `xfree(v)`, which only
                // frees `v`'s own memory block and never touches
                // whatever `v->di_tv` itself references) - forget the
                // old value first so it is genuinely left alone,
                // matching the original's free_val=false contract
                // exactly.
                // SAFETY: forwarded from this function's own safety doc.
                let old = unsafe { std::mem::take(&mut (*item).di_tv) };
                std::mem::forget(old);
            }
            // SAFETY: forwarded from this function's own safety doc.
            drop(unsafe { Box::from_raw(item) });
        } else if free_val {
            // Not separately allocated (embedded elsewhere, e.g. a
            // ScopeDictDictItem) and staying alive - after
            // tv_clear_simple above released any pointer-based ref,
            // explicitly reset di_tv to a clean Default, exactly
            // mirroring tv_dict_item_free's own already-established
            // non-ALLOC branch: the assignment's implicit drop of the
            // OLD di_tv releases any owned String/Vec bytes
            // tv_clear_simple itself intentionally leaves for Rust's
            // normal drop to handle.
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { (*item).di_tv = TypvalT::default() };
        }
        // Non-ALLOC + free_val=false: nothing happens to this item at
        // all, matching the original exactly (neither tv_clear nor
        // xfree runs).
    }
    d.dv_index.clear();
    d.dv_hashtab = crate::hashtab_defs::HashtabT::hash_init();
}

/// Clean up a list of internal variables: frees all allocated
/// variables and the value they contain, and clears `d`'s own
/// hashtab (`vars_clear`). See [`vars_clear_ext`]'s own doc comment
/// for why this takes `&mut DictT` rather than the original's bare
/// `&mut hashtab_T`.
///
/// # Safety
/// Same as [`vars_clear_ext`].
pub unsafe fn vars_clear(d: &mut DictT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { vars_clear_ext(d, true) };
}

#[cfg(test)]
mod set_vcount_and_valid_varname_tests {
    use super::*;

    #[test]
    fn set_vcount_sets_count_and_count1_without_prevcount() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { set_vim_var_nr(VimVarIndex::Prevcount, 0) };
        unsafe { set_vcount(5, 6, false) };
        assert_eq!(unsafe { get_vim_var_nr(VimVarIndex::Count) }, 5);
        assert_eq!(unsafe { get_vim_var_nr(VimVarIndex::Count1) }, 6);
        assert_eq!(unsafe { get_vim_var_nr(VimVarIndex::Prevcount) }, 0);
    }

    #[test]
    fn set_vcount_copies_old_count_into_prevcount_when_requested() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { set_vim_var_nr(VimVarIndex::Count, 3) };
        unsafe { set_vcount(7, 8, true) };
        // prevcount picks up the OLD v:count (3), not the new one (7).
        assert_eq!(unsafe { get_vim_var_nr(VimVarIndex::Prevcount) }, 3);
        assert_eq!(unsafe { get_vim_var_nr(VimVarIndex::Count) }, 7);
        assert_eq!(unsafe { get_vim_var_nr(VimVarIndex::Count1) }, 8);
    }

    #[test]
    fn valid_varname_empty_is_true() {
        assert!(valid_varname(b""));
    }

    #[test]
    fn valid_varname_plain_identifier_is_true() {
        assert!(valid_varname(b"foo"));
        assert!(valid_varname(b"_foo"));
        assert!(valid_varname(b"foo123"));
    }

    #[test]
    fn valid_varname_digit_at_start_is_false() {
        assert!(!valid_varname(b"123foo"));
    }

    #[test]
    fn valid_varname_autoload_char_allowed_anywhere_including_start() {
        assert!(valid_varname(b"foo#bar"));
        assert!(valid_varname(b"#foo"));
    }

    #[test]
    fn valid_varname_rejects_other_punctuation() {
        assert!(!valid_varname(b"foo-bar"));
        assert!(!valid_varname(b"foo bar"));
    }

    #[test]
    fn var_check_ro_true_when_readonly_flag_set() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(var_check_ro(dict_item_flags::RO));
    }

    #[test]
    fn var_check_ro_false_for_plain_flags() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;
        assert!(!var_check_ro(0));
        assert!(!var_check_ro(dict_item_flags::FIX));
    }

    #[test]
    fn var_check_ro_sandbox_flag_only_blocks_inside_the_sandbox() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;
        assert!(!var_check_ro(dict_item_flags::RO_SBX));
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 1;
        assert!(var_check_ro(dict_item_flags::RO_SBX));
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;
    }

    #[test]
    fn var_check_lock_reflects_the_lock_flag() {
        assert!(var_check_lock(dict_item_flags::LOCK));
        assert!(!var_check_lock(dict_item_flags::RO));
        assert!(!var_check_lock(0));
    }

    #[test]
    fn var_check_fixed_reflects_the_fix_flag() {
        assert!(var_check_fixed(dict_item_flags::FIX));
        assert!(!var_check_fixed(dict_item_flags::LOCK));
        assert!(!var_check_fixed(0));
    }
}

#[cfg(test)]
mod globvardict_tests {
    use super::*;
    use crate::eval::typval::tv_dict_add;

    /// Every test here must leave `GLOBVARDICT` empty again afterward -
    /// it is a genuinely shared, persistent static (unlike a
    /// `tv_dict_alloc()`-allocated dict a test can freely free), so
    /// stale entries would otherwise leak across tests. Reuses this
    /// module's own real `vars_clear`, dogfooding it the same way
    /// `add_nr_var`'s own test already exercises `tv_dict_add`.
    fn reset_globvardict() {
        unsafe { vars_clear(GLOBVARDICT.get_mut()) };
    }

    #[test]
    fn get_globvar_dict_returns_a_usable_pointer_to_the_shared_globvardict() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globvardict();
        let d = get_globvar_dict();
        assert!(!d.is_null());
        assert_eq!(unsafe { (*d).dv_index.len() }, 0);
        // Same underlying storage every call - not a fresh allocation.
        assert_eq!(get_globvar_dict(), d);
        reset_globvardict();
    }

    #[test]
    fn del_menutrans_vars_removes_only_menutrans_prefixed_entries() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globvardict();
        let d = get_globvar_dict();
        let menu_item = crate::eval::typval::tv_dict_item_alloc(b"menutrans_File");
        let other_item = crate::eval::typval::tv_dict_item_alloc(b"other_var");
        unsafe { tv_dict_add(&mut *d, menu_item) };
        unsafe { tv_dict_add(&mut *d, other_item) };
        assert_eq!(unsafe { (*d).dv_index.len() }, 2);

        del_menutrans_vars();

        assert_eq!(unsafe { (*d).dv_index.len() }, 1);
        assert!(crate::eval::typval::tv_dict_find(Some(unsafe { &mut *d }), b"other_var").is_some());
        assert!(crate::eval::typval::tv_dict_find(Some(unsafe { &mut *d }), b"menutrans_File").is_none());

        reset_globvardict();
    }

    #[test]
    fn del_menutrans_vars_is_a_noop_when_nothing_matches() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globvardict();
        let d = get_globvar_dict();
        let item = crate::eval::typval::tv_dict_item_alloc(b"other_var");
        unsafe { tv_dict_add(&mut *d, item) };

        del_menutrans_vars();

        assert_eq!(unsafe { (*d).dv_index.len() }, 1);
        reset_globvardict();
    }

    #[test]
    fn garbage_collect_globvars_marks_a_nested_dict_reachable_from_g() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globvardict();
        let d = get_globvar_dict();
        let nested = crate::eval::typval::tv_dict_alloc();
        let item = crate::eval::typval::tv_dict_item_alloc(b"nested");
        unsafe { (*item).di_tv.value = TypvalValue::Dict(nested) };
        unsafe { tv_dict_add(&mut *d, item) };

        let aborted = unsafe { garbage_collect_globvars(13) };

        assert!(!aborted);
        assert_eq!(unsafe { (*nested).dv_copy_id }, 13);

        reset_globvardict();
    }

    #[test]
    fn garbage_collect_globvars_false_when_globvardict_empty() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globvardict();
        assert!(!unsafe { garbage_collect_globvars(1) });
    }

    #[test]
    fn garbage_collect_scriptvars_false_when_no_scripts_registered() {
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        assert!(!unsafe { garbage_collect_scriptvars(1) });
    }

    #[test]
    fn garbage_collect_scriptvars_marks_a_nested_dict_reachable_from_a_scripts_own_s_scope() {
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        let (sid, _) = crate::runtime::new_script_item(None);
        let item_ptr = crate::runtime::script_item(sid);
        let sv = unsafe { (*item_ptr).sn_vars };

        let nested = crate::eval::typval::tv_dict_alloc();
        let item = crate::eval::typval::tv_dict_item_alloc(b"nested");
        unsafe { (*item).di_tv.value = TypvalValue::Dict(nested) };
        unsafe { tv_dict_add(&mut (*sv).sv_dict, item) };

        let aborted = unsafe { garbage_collect_scriptvars(9) };

        assert!(!aborted);
        assert_eq!(unsafe { (*nested).dv_copy_id }, 9);

        crate::runtime::tests_reset_for_test();
    }
}

#[cfg(test)]
mod unref_var_dict_and_vars_clear_tests {
    use super::*;
    use crate::eval::typval::{tv_dict_add, tv_dict_alloc, tv_dict_free, tv_dict_item_alloc, tv_list_alloc, tv_list_ref};

    #[test]
    fn unref_var_dict_frees_when_transitioning_from_do_not_free_cnt_to_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe { (*d).dv_refcount = DO_NOT_FREE_CNT };
        // Refcount lands at exactly 0 after the transition + real
        // unref - the real free path runs to completion. Nothing
        // further to assert on `d` after this - the absence of a
        // crash is the check (matching this crate's own
        // func_ptr_unref_frees_when_hits_zero_and_not_being_called
        // precedent).
        unsafe { unref_var_dict(d) };
    }

    #[test]
    fn unref_var_dict_survives_when_still_referenced_elsewhere() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        // One extra reference beyond the DO_NOT_FREE_CNT sentinel -
        // simulates something else also holding a real reference.
        unsafe { (*d).dv_refcount = DO_NOT_FREE_CNT + 1 };
        unsafe { unref_var_dict(d) };
        assert_eq!(unsafe { (*d).dv_refcount }, 1);
        unsafe { tv_dict_free(d) };
    }

    #[test]
    fn vars_clear_ext_true_frees_allocated_items_and_empties_the_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        let item = tv_dict_item_alloc(b"x");
        unsafe { (*item).di_tv.value = TypvalValue::Number(42) };
        unsafe { tv_dict_add(&mut *d, item) };

        unsafe { vars_clear_ext(&mut *d, true) };

        assert_eq!(unsafe { (*d).dv_index.len() }, 0);
        assert_eq!(unsafe { (*d).dv_hashtab.ht_used }, 0);
        unsafe { tv_dict_free(d) };
    }

    #[test]
    fn vars_clear_ext_true_releases_a_list_reference_the_item_holds() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        let list = tv_list_alloc(0);
        unsafe { tv_list_ref(list) }; // matches a real List-typed di_tv's own +1 ref
        let item = tv_dict_item_alloc(b"l");
        unsafe { (*item).di_tv.value = TypvalValue::List(list) };
        unsafe { tv_dict_add(&mut *d, item) };
        assert_eq!(unsafe { (*list).lv_refcount }, 1);

        unsafe { vars_clear_ext(&mut *d, true) };

        // The list's own reference was released - refcount dropped to
        // 0, freeing it. Nothing further to assert on `list` itself -
        // matches this crate's own established "absence of a crash is
        // the check" precedent for a hits-zero-and-frees path.
        unsafe { tv_dict_free(d) };
    }

    #[test]
    fn vars_clear_ext_false_does_not_release_a_list_reference_the_item_holds() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        let list = tv_list_alloc(0);
        unsafe { tv_list_ref(list) };
        unsafe { tv_list_ref(list) }; // a 2nd ref this test itself owns, to keep `list` alive
        let item = tv_dict_item_alloc(b"l");
        unsafe { (*item).di_tv.value = TypvalValue::List(list) };
        unsafe { tv_dict_add(&mut *d, item) };
        assert_eq!(unsafe { (*list).lv_refcount }, 2);

        unsafe { vars_clear_ext(&mut *d, false) };

        // free_val=false: the list reference the item held is left
        // completely untouched (not released) - refcount is
        // unchanged, this test's own extra ref is still valid.
        assert_eq!(unsafe { (*list).lv_refcount }, 2);
        assert_eq!(unsafe { (*d).dv_index.len() }, 0);

        // Release both remaining refs directly to clean up.
        unsafe { crate::eval::typval::tv_list_unref(list) };
        unsafe { crate::eval::typval::tv_list_unref(list) };
        unsafe { tv_dict_free(d) };
    }

    #[test]
    fn vars_clear_delegates_to_vars_clear_ext_with_free_val_true() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        let list = tv_list_alloc(0);
        unsafe { tv_list_ref(list) };
        let item = tv_dict_item_alloc(b"l");
        unsafe { (*item).di_tv.value = TypvalValue::List(list) };
        unsafe { tv_dict_add(&mut *d, item) };

        unsafe { vars_clear(&mut *d) };

        assert_eq!(unsafe { (*d).dv_index.len() }, 0);
        unsafe { tv_dict_free(d) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vimvars_table_has_exactly_108_entries() {
        let _lock = crate::globals::global_state_test_lock();
        // SAFETY: forwarded from get_vim_var_tv's own established
        // GlobalCell convention.
        assert_eq!(unsafe { VIMVARS.get_mut() }.len(), 108);
    }

    #[test]
    fn vimvars_table_spot_check_names_and_types_including_the_reordered_pair() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(get_vim_var_name(VimVarIndex::Count), "count");
        assert_eq!(get_vim_var_name(VimVarIndex::Startreason), "startreason");
        // VV_TERMREQUEST/VV_TERMRESPONSE: declared in one order in the
        // enum but the OPPOSITE order in the table's own file text -
        // confirms indexing follows the enum, not file order (see
        // this module's own doc comment).
        assert_eq!(get_vim_var_name(VimVarIndex::Termrequest), "termrequest");
        assert_eq!(get_vim_var_name(VimVarIndex::Termresponse), "termresponse");
        // SAFETY: forwarded from get_vim_var_tv's own established
        // GlobalCell convention.
        unsafe {
            assert_eq!((*get_vim_var_tv(VimVarIndex::Val)).value.var_type(), VarType::Unknown);
            assert_eq!((*get_vim_var_tv(VimVarIndex::False)).value.var_type(), VarType::Bool);
            assert_eq!((*get_vim_var_tv(VimVarIndex::Null)).value.var_type(), VarType::Special);
            assert_eq!((*get_vim_var_tv(VimVarIndex::Lua)).value.var_type(), VarType::Partial);
            assert_eq!((*get_vim_var_tv(VimVarIndex::Oldfiles)).value.var_type(), VarType::List);
            assert_eq!(
                (*get_vim_var_tv(VimVarIndex::CompletedItem)).value.var_type(),
                VarType::Dict
            );
        }
    }

    #[test]
    fn get_vim_var_nr_default_is_zero_for_a_number_typed_slot() {
        let _lock = crate::globals::global_state_test_lock();
        // SAFETY: forwarded from get_vim_var_tv's own established
        // GlobalCell convention.
        assert_eq!(unsafe { get_vim_var_nr(VimVarIndex::ShellError) }, 0);
    }

    #[test]
    #[should_panic(expected = "is not Number-typed")]
    fn get_vim_var_nr_panics_on_a_non_number_slot() {
        let _lock = crate::globals::global_state_test_lock();
        // SAFETY: forwarded from get_vim_var_tv's own established
        // GlobalCell convention.
        let _ = unsafe { get_vim_var_nr(VimVarIndex::Errmsg) };
    }

    #[test]
    fn get_vim_var_str_default_is_empty_for_an_unset_string_slot() {
        let _lock = crate::globals::global_state_test_lock();
        // SAFETY: forwarded from get_vim_var_tv's own established
        // GlobalCell convention.
        assert_eq!(unsafe { get_vim_var_str(VimVarIndex::Warningmsg) }, Vec::<u8>::new());
    }

    #[test]
    fn set_vim_var_nr_and_get_vim_var_nr_roundtrip() {
        let _lock = crate::globals::global_state_test_lock();
        // SAFETY: forwarded from get_vim_var_tv's own established
        // GlobalCell convention.
        unsafe {
            set_vim_var_nr(VimVarIndex::Cmdbang, 42);
            assert_eq!(get_vim_var_nr(VimVarIndex::Cmdbang), 42);
            // Reset: VIMVARS is shared, process-wide state - leave it
            // as found so no other test observes this mutation.
            set_vim_var_nr(VimVarIndex::Cmdbang, 0);
        }
    }

    #[test]
    fn set_vim_var_type_number_then_set_vim_var_nr_matches_vv_key_vv_val_usage() {
        // Mirrors the real eval/funcs.c usage pattern for VV_KEY/VV_VAL:
        // starts VAR_UNKNOWN, set_vim_var_type(..., VAR_NUMBER) then
        // set_vim_var_nr(...) turns it into a real Number.
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            assert_eq!((*get_vim_var_tv(VimVarIndex::Val)).value.var_type(), VarType::Unknown);
            set_vim_var_type(VimVarIndex::Val, VarType::Number);
            set_vim_var_nr(VimVarIndex::Val, 7);
            assert_eq!(get_vim_var_nr(VimVarIndex::Val), 7);
            // Reset: VIMVARS is shared, process-wide state - restore
            // Val's own true static-initializer default (Unknown) so
            // no other test (e.g. the spot-check test) observes this
            // permanent type change.
            set_vim_var_tv(VimVarIndex::Val, TypvalT::default());
        }
    }

    #[test]
    #[should_panic(expected = "only VarType::Number")]
    fn set_vim_var_type_panics_for_non_number_type() {
        let _lock = crate::globals::global_state_test_lock();
        // SAFETY: forwarded from get_vim_var_tv's own established
        // GlobalCell convention. Panics before ever writing to Key's
        // slot (set_vim_var_type's own assert runs first), so no
        // cross-test state leakage here despite the panic.
        unsafe { set_vim_var_type(VimVarIndex::Key, VarType::String) };
    }

    #[test]
    fn set_vim_var_bool_roundtrip() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            set_vim_var_bool(VimVarIndex::False, BoolVarValue::True);
            assert!(matches!(
                (*get_vim_var_tv(VimVarIndex::False)).value,
                TypvalValue::Bool(BoolVarValue::True)
            ));
            // Reset: VIMVARS is shared, process-wide state - restore
            // False's own true static-initializer default.
            set_vim_var_bool(VimVarIndex::False, BoolVarValue::False);
        }
    }

    #[test]
    fn set_vim_var_special_roundtrip() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            set_vim_var_special(VimVarIndex::FcsChoice, SpecialVarValue::Null);
            assert!(matches!(
                (*get_vim_var_tv(VimVarIndex::FcsChoice)).value,
                TypvalValue::Special(SpecialVarValue::Null)
            ));
            // Reset: VIMVARS is shared, process-wide state - restore
            // FcsChoice's own true static-initializer default
            // (String(None), NOT TypvalT::default()'s Unknown - the
            // vimvars table declares each slot's own DIFFERENT default
            // type, so a blanket Default::default() would be wrong
            // here; see VIMVARS' own construction for the real value).
            set_vim_var_tv(
                VimVarIndex::FcsChoice,
                TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) },
            );
        }
    }

    #[test]
    fn set_vim_var_string_and_get_vim_var_str_roundtrip() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            set_vim_var_string(VimVarIndex::Progname, Some(b"nero"));
            assert_eq!(get_vim_var_str(VimVarIndex::Progname), b"nero");
            // Reset: VIMVARS is shared, process-wide state.
            set_vim_var_string(VimVarIndex::Progname, None);
        }
    }

    #[test]
    fn set_vim_var_string_none_clears_to_empty() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            set_vim_var_string(VimVarIndex::Progpath, Some(b"x"));
            set_vim_var_string(VimVarIndex::Progpath, None);
            assert_eq!(get_vim_var_str(VimVarIndex::Progpath), Vec::<u8>::new());
        }
    }

    #[test]
    fn set_vim_var_list_increments_refcount_and_stores_pointer() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let l = crate::eval::typval::tv_list_alloc(0);
            assert_eq!((*l).lv_refcount, 0);
            set_vim_var_list(VimVarIndex::Oldfiles, l);
            assert_eq!((*l).lv_refcount, 1);
            assert_eq!(get_vim_var_list(VimVarIndex::Oldfiles), l);
            crate::eval::typval::tv_list_unref(l);
            // Reset: VIMVARS is shared, process-wide state - tv_list_unref
            // just freed `l`, so Oldfiles' slot is now a DANGLING
            // pointer; restore its own true static-initializer default
            // (a null List, NOT TypvalT::default()'s Unknown - see
            // set_vim_var_special_roundtrip's own comment above on why
            // a blanket Default::default() would be wrong here) so no
            // other test/future feature can ever read that freed
            // memory.
            set_vim_var_tv(
                VimVarIndex::Oldfiles,
                TypvalT {
                    v_lock: VarLockStatus::Unlocked,
                    value: TypvalValue::List(std::ptr::null_mut()),
                },
            );
        }
    }

    #[test]
    fn set_vim_var_dict_increments_refcount_locks_keys_and_stores_pointer() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let d = crate::eval::typval::tv_dict_alloc();
            let item = crate::eval::typval::tv_dict_item_alloc(b"x");
            assert_eq!(crate::eval::typval::tv_dict_add(&mut *d, item), crate::vim_defs::OK);
            assert_eq!((*d).dv_refcount, 0);

            set_vim_var_dict(VimVarIndex::CompletedItem, d);

            assert_eq!((*d).dv_refcount, 1);
            assert_eq!(get_vim_var_dict(VimVarIndex::CompletedItem), d);
            // Keys made read-only (DI_FLAGS_RO|DI_FLAGS_FIX), on top of
            // tv_dict_item_alloc's own pre-existing DI_FLAGS_ALLOC.
            assert_eq!(
                (*item).di_flags,
                dict_item_flags::ALLOC | dict_item_flags::RO | dict_item_flags::FIX
            );

            crate::eval::typval::tv_dict_unref(d);
            // Reset: VIMVARS is shared, process-wide state -
            // tv_dict_unref just freed `d`, so CompletedItem's slot is
            // now a DANGLING pointer; restore its own true
            // static-initializer default (a null Dict, NOT
            // TypvalT::default()'s Unknown - see
            // set_vim_var_special_roundtrip's own comment above),
            // matching set_vim_var_list's own established reset
            // precedent above.
            set_vim_var_tv(
                VimVarIndex::CompletedItem,
                TypvalT {
                    v_lock: VarLockStatus::Unlocked,
                    value: TypvalValue::Dict(std::ptr::null_mut()),
                },
            );
        }
    }

    #[test]
    fn set_vim_var_dict_null_is_a_safe_noop_after_storing() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            set_vim_var_dict(VimVarIndex::Event, std::ptr::null_mut());
            assert!(get_vim_var_dict(VimVarIndex::Event).is_null());
        }
    }

    #[test]
    fn set_vim_var_partial_roundtrip() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let mut p = crate::eval::typval_defs::PartialT::default();
            set_vim_var_partial(VimVarIndex::Lua, &mut p as *mut _);
            assert_eq!(get_vim_var_partial(VimVarIndex::Lua), &mut p as *mut _);
            set_vim_var_partial(VimVarIndex::Lua, std::ptr::null_mut());
        }
    }

    #[test]
    fn set_vim_var_char_stores_the_encoded_character() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            set_vim_var_char(i32::from(b'q'));
            assert_eq!(get_vim_var_str(VimVarIndex::Char), b"q");
        }
    }

    #[test]
    fn set_reg_var_stores_quote_for_zero_or_space() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            set_reg_var(0);
            assert_eq!(get_vim_var_str(VimVarIndex::Reg), b"\"");
            set_reg_var(i32::from(b' '));
            assert_eq!(get_vim_var_str(VimVarIndex::Reg), b"\"");
        }
    }

    #[test]
    fn set_reg_var_stores_the_given_register_name() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            set_reg_var(i32::from(b'a'));
            assert_eq!(get_vim_var_str(VimVarIndex::Reg), b"a");
        }
    }

    #[test]
    fn init_var_dict_wires_dict_var_to_point_at_dict() {
        let mut dict = DictT {
            dv_lock: VarLockStatus::Locked,
            dv_scope: ScopeType::NoScope,
            dv_refcount: 999,
            dv_copy_id: 5,
            dv_hashtab: crate::hashtab_defs::HashtabT::hash_init(),
            dv_index: std::collections::HashMap::new(),
            dv_copydict: std::ptr::null_mut(),
            dv_used_next: std::ptr::null_mut(),
            dv_used_prev: std::ptr::null_mut(),
            lua_table_ref: -1,
        };
        let mut dict_var = ScopeDictDictItem::default();

        init_var_dict(&mut dict, &mut dict_var, ScopeType::Scope);

        assert_eq!(dict.dv_lock, VarLockStatus::Unlocked);
        assert_eq!(dict.dv_scope, ScopeType::Scope);
        assert_eq!(dict.dv_refcount, DO_NOT_FREE_CNT);
        assert_eq!(dict.dv_copy_id, 0);

        assert_eq!(dict_var.di_tv.v_lock, VarLockStatus::Fixed);
        assert_eq!(
            dict_var.di_flags,
            dict_item_flags::RO | dict_item_flags::FIX
        );
        assert_eq!(dict_var.di_key, vec![0]);
        match dict_var.di_tv.value {
            TypvalValue::Dict(p) => assert_eq!(p, &mut dict as *mut DictT),
            _ => panic!("expected a Dict-typed value"),
        }
    }

    #[test]
    fn init_var_dict_matches_def_scope_too() {
        let mut dict = DictT {
            dv_lock: VarLockStatus::Unlocked,
            dv_scope: ScopeType::NoScope,
            dv_refcount: 0,
            dv_copy_id: 0,
            dv_hashtab: crate::hashtab_defs::HashtabT::hash_init(),
            dv_index: std::collections::HashMap::new(),
            dv_copydict: std::ptr::null_mut(),
            dv_used_next: std::ptr::null_mut(),
            dv_used_prev: std::ptr::null_mut(),
            lua_table_ref: -1,
        };
        let mut dict_var = ScopeDictDictItem::default();

        init_var_dict(&mut dict, &mut dict_var, ScopeType::DefScope);

        assert_eq!(dict.dv_scope, ScopeType::DefScope);
    }

    // The following tests all touch crate::runtime's shared
    // SCRIPT_ITEMS/LAST_CURRENT_SID GlobalCells (indirectly, through
    // new_script_vars's own call to crate::runtime::script_item) -
    // each acquires global_state_test_lock() for its whole body and
    // resets crate::runtime's test-only state first, matching
    // crate::runtime's own test conventions exactly.

    #[test]
    fn new_script_vars_wires_a_fresh_scope_dict_into_the_script_item() {
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        let (sid, item) = crate::runtime::new_script_item(None);
        // new_script_item already called new_script_vars(sid) once as
        // part of allocating the slot - call it again directly to
        // exercise this function's own behavior in isolation too
        // (mirrors init_var_dict's own "call it again with different
        // inputs" test style above).
        new_script_vars(sid);
        unsafe {
            assert!(!(*item).sn_vars.is_null());
            let sv = &*(*item).sn_vars;
            assert_eq!(sv.sv_dict.dv_scope, ScopeType::Scope);
            assert_eq!(sv.sv_dict.dv_refcount, DO_NOT_FREE_CNT);
            assert_eq!(sv.sv_dict.dv_lock, VarLockStatus::Unlocked);
            assert!(sv.sv_dict.dv_used_next.is_null());
            assert!(sv.sv_dict.dv_used_prev.is_null());
            match sv.sv_var.di_tv.value {
                TypvalValue::Dict(p) => assert_eq!(p, &sv.sv_dict as *const DictT as *mut DictT),
                _ => panic!("expected a Dict-typed value"),
            }
        }
    }

    #[test]
    #[should_panic]
    fn new_script_vars_panics_for_out_of_range_sid() {
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        new_script_vars(42);
    }
}

#[cfg(test)]
mod vimvardict_tests {
    use super::*;
    use crate::eval::typval::tv_dict_find;

    #[test]
    fn vimvars_di_key_and_di_flags_are_populated_from_name_and_flags() {
        let _lock = crate::globals::global_state_test_lock();
        // SAFETY: forwarded from get_vim_var_tv's own established
        // GlobalCell convention.
        let vimvars = unsafe { VIMVARS.get_mut() };
        // VV_COUNT: RO.
        let count = &vimvars[VimVarIndex::Count as usize];
        assert_eq!(count.di.di_key, b"count\0");
        assert_eq!(count.di.di_flags, dict_item_flags::RO | dict_item_flags::FIX);
        // VV_ERRMSG: no flags at all.
        let errmsg = &vimvars[VimVarIndex::Errmsg as usize];
        assert_eq!(errmsg.di.di_key, b"errmsg\0");
        assert_eq!(errmsg.di.di_flags, dict_item_flags::FIX);
        // VV_LNUM: RO_SBX.
        let lnum = &vimvars[VimVarIndex::Lnum as usize];
        assert_eq!(lnum.di.di_key, b"lnum\0");
        assert_eq!(lnum.di.di_flags, dict_item_flags::RO_SBX | dict_item_flags::FIX);
    }

    #[test]
    fn get_vimvar_dict_returns_a_stable_pointer() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = get_vimvar_dict();
        let d2 = get_vimvar_dict();
        assert!(!d1.is_null());
        assert_eq!(d1, d2);
    }

    #[test]
    fn get_vimvar_dict_contains_every_entry_except_val_and_key() {
        let _lock = crate::globals::global_state_test_lock();
        // 108 entries total, minus VV_VAL/VV_KEY (VAR_UNKNOWN at
        // construction time - see VIMVARS's own doc comment).
        assert_eq!(unsafe { (*get_vimvar_dict()).dv_index.len() }, 106);
        assert!(tv_dict_find(unsafe { get_vimvar_dict().as_mut() }, b"val").is_none());
        assert!(tv_dict_find(unsafe { get_vimvar_dict().as_mut() }, b"key").is_none());
        assert!(tv_dict_find(unsafe { get_vimvar_dict().as_mut() }, b"count").is_some());
        assert!(tv_dict_find(unsafe { get_vimvar_dict().as_mut() }, b"version").is_some());
    }

    #[test]
    fn get_vimvar_dict_aliases_the_same_storage_as_get_vim_var_nr() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { set_vim_var_nr(VimVarIndex::Count, 42) };

        let di = tv_dict_find(unsafe { get_vimvar_dict().as_mut() }, b"count")
            .expect("count must be a pre-populated entry");
        assert_eq!(unsafe { &(*di).di_tv.value }, &TypvalValue::Number(42));

        // Mutating through the dict-item pointer must be visible via
        // get_vim_var_nr too - this is the SAME storage, not a
        // synchronized copy (see VIMVARDICT's own doc comment).
        unsafe { (*di).di_tv.value = TypvalValue::Number(7) };
        assert_eq!(unsafe { get_vim_var_nr(VimVarIndex::Count) }, 7);

        // Restore VV_COUNT's default so other tests aren't affected.
        unsafe { set_vim_var_nr(VimVarIndex::Count, 0) };
    }

    #[test]
    fn compat_hashtab_contains_only_the_version_entry() {
        let _lock = crate::globals::global_state_test_lock();
        // SAFETY: forwarded from get_vim_var_tv's own established
        // GlobalCell convention.
        let ht = unsafe { COMPAT_HASHTAB.get_mut() };
        assert!(!hashitem_empty(ht.hash_find(b"version")));
        // "count" is a real vimvar but NOT VV_COMPAT-flagged, so it
        // must not be implicitly reachable without "v:".
        assert!(hashitem_empty(ht.hash_find(b"count")));
    }

    #[test]
    fn garbage_collect_vimvars_false_when_nothing_extra_reachable() {
        let _lock = crate::globals::global_state_test_lock();
        // Every VIMVARS entry defaults to a scalar/null value - no
        // nested container reachable from v: means nothing for the
        // mark phase to abort over.
        assert!(!unsafe { garbage_collect_vimvars(1) });
    }
}

#[cfg(test)]
mod find_var_ht_dict_tests {
    use super::*;
    use crate::eval::typval_defs::FunccallT;

    /// Sets `GLOBALS.curbuf`/`curwin`/`curtab` to freshly-boxed,
    /// plain-`Default` structs (no `ml_open`/memline setup needed -
    /// `find_var_ht_dict`'s `b:`/`w:`/`t:` branches only ever read
    /// `b_vars`/`w_vars`/`tp_vars` directly), restoring the previous
    /// values and freeing everything on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's entire lifetime,
    /// matching `undo.rs`'s own `TestBufWin` established convention
    /// for the same kind of curbuf/curwin RAII setup (extended here to
    /// also cover curtab, which that helper doesn't need).
    struct TestCurBufWinTab {
        buf: *mut crate::buffer_defs::BufT,
        win: *mut crate::buffer_defs::WinT,
        tab: *mut crate::buffer_defs::TabpageT,
        prev_curbuf: *mut crate::buffer_defs::BufT,
        prev_curwin: *mut crate::buffer_defs::WinT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
    }

    impl TestCurBufWinTab {
        fn new() -> Self {
            let buf = Box::into_raw(Box::new(crate::buffer_defs::BufT::default()));
            let win = Box::into_raw(Box::new(crate::buffer_defs::WinT::default()));
            let tab = Box::into_raw(Box::new(crate::buffer_defs::TabpageT::default()));
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let prev_curbuf = g.curbuf;
            let prev_curwin = g.curwin;
            let prev_curtab = g.curtab;
            g.curbuf = buf;
            g.curwin = win;
            g.curtab = tab;
            TestCurBufWinTab { buf, win, tab, prev_curbuf, prev_curwin, prev_curtab }
        }
    }

    impl Drop for TestCurBufWinTab {
        fn drop(&mut self) {
            unsafe {
                let g = crate::globals::GLOBALS.get_mut();
                g.curbuf = self.prev_curbuf;
                g.curwin = self.prev_curwin;
                g.curtab = self.prev_curtab;
                drop(Box::from_raw(self.buf));
                drop(Box::from_raw(self.win));
                drop(Box::from_raw(self.tab));
            }
        }
    }

    /// Every test here must leave `CURRENT_FUNCCAL`/`GLOBVARDICT`/
    /// script items reset, matching this crate's established
    /// test-isolation discipline for shared `GlobalCell` state.
    /// `COMPAT_HASHTAB`/`VIMVARDICT`/`VIMVARS` are deliberately NOT
    /// reset here - like `VIMVARS` itself (never reset - see its own
    /// doc comment), they are populate-once, process-lifetime
    /// structures now that both self-populate from `VIMVARS` on first
    /// access; no test in this module ever mutates them, only reads
    /// via `find_var_ht_dict`, so resetting would just permanently
    /// (and wrongly) empty them after the first test that ran this
    /// function, since `LazyLock`'s init closure never re-runs.
    fn reset_shared_state() {
        crate::eval::userfunc::set_current_funccal(std::ptr::null_mut());
        unsafe { vars_clear(GLOBVARDICT.get_mut()) };
        crate::runtime::tests_reset_for_test();
        unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx = Default::default();
    }

    #[test]
    fn find_var_ht_dict_empty_name_returns_null() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (ht, varname, d) = find_var_ht_dict(b"");
        assert!(ht.is_null());
        assert!(d.is_null());
        assert_eq!(varname, b"");
    }

    #[test]
    fn find_var_ht_dict_rejects_leading_colon_or_hash() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        assert!(find_var_ht_dict(b":foo").0.is_null());
        assert!(find_var_ht_dict(b"#foo").0.is_null());
    }

    #[test]
    fn find_var_ht_dict_implicit_scope_falls_back_to_globvar() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (ht, varname, d) = find_var_ht_dict(b"foo");
        assert_eq!(varname, b"foo");
        assert_eq!(d, get_globvar_dict());
        assert_eq!(ht, unsafe { &mut (*d).dv_hashtab as *mut HashtabT });
        reset_shared_state();
    }

    #[test]
    fn find_var_ht_dict_implicit_scope_prefers_funccal_local_when_present() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let mut fc = Box::new(FunccallT::default());
        fc.fc_l_vars.dv_refcount = DO_NOT_FREE_CNT;
        let fc_ptr = fc.as_mut() as *mut FunccallT;
        crate::eval::userfunc::set_current_funccal(fc_ptr);

        let (ht, varname, d) = find_var_ht_dict(b"foo");
        assert_eq!(varname, b"foo");
        assert_eq!(d, unsafe { &mut (*fc_ptr).fc_l_vars as *mut DictT });
        assert_eq!(ht, unsafe { &mut (*fc_ptr).fc_l_vars.dv_hashtab as *mut HashtabT });

        crate::eval::userfunc::set_current_funccal(std::ptr::null_mut());
    }

    #[test]
    fn find_var_ht_dict_g_colon_resolves_to_globvar_dict() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (ht, varname, d) = find_var_ht_dict(b"g:foo");
        assert_eq!(varname, b"foo");
        assert_eq!(d, get_globvar_dict());
        assert!(!ht.is_null());
        reset_shared_state();
    }

    #[test]
    fn find_var_ht_dict_rejects_extra_colon_or_hash_without_g_prefix() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        assert!(find_var_ht_dict(b"b:foo:bar").0.is_null());
        assert!(find_var_ht_dict(b"b:foo#bar").0.is_null());
        // But "g:" itself is exempt from this check.
        assert!(!find_var_ht_dict(b"g:foo:bar").0.is_null());
        reset_shared_state();
    }

    #[test]
    fn find_var_ht_dict_b_colon_resolves_to_curbuf_b_vars() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let cbwt = TestCurBufWinTab::new();
        let d = crate::eval::typval::tv_dict_alloc();
        unsafe { (*cbwt.buf).b_vars = d };

        let (ht, varname, found_d) = find_var_ht_dict(b"b:foo");
        assert_eq!(varname, b"foo");
        assert_eq!(found_d, d);
        assert_eq!(ht, unsafe { &mut (*d).dv_hashtab as *mut HashtabT });

        unsafe {
            (*cbwt.buf).b_vars = std::ptr::null_mut();
            crate::eval::typval::tv_dict_free(d);
        }
    }

    #[test]
    fn find_var_ht_dict_w_colon_resolves_to_curwin_w_vars() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let cbwt = TestCurBufWinTab::new();
        let d = crate::eval::typval::tv_dict_alloc();
        unsafe { (*cbwt.win).w_vars = d };

        let (ht, varname, found_d) = find_var_ht_dict(b"w:foo");
        assert_eq!(varname, b"foo");
        assert_eq!(found_d, d);
        assert_eq!(ht, unsafe { &mut (*d).dv_hashtab as *mut HashtabT });

        unsafe {
            (*cbwt.win).w_vars = std::ptr::null_mut();
            crate::eval::typval::tv_dict_free(d);
        }
    }

    #[test]
    fn find_var_ht_dict_t_colon_resolves_to_curtab_tp_vars() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let cbwt = TestCurBufWinTab::new();
        let d = crate::eval::typval::tv_dict_alloc();
        unsafe { (*cbwt.tab).tp_vars = d };

        let (ht, varname, found_d) = find_var_ht_dict(b"t:foo");
        assert_eq!(varname, b"foo");
        assert_eq!(found_d, d);
        assert_eq!(ht, unsafe { &mut (*d).dv_hashtab as *mut HashtabT });

        unsafe {
            (*cbwt.tab).tp_vars = std::ptr::null_mut();
            crate::eval::typval::tv_dict_free(d);
        }
    }

    #[test]
    fn find_var_ht_dict_v_colon_resolves_to_vimvar_dict() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (ht, varname, d) = find_var_ht_dict(b"v:count");
        assert_eq!(varname, b"count");
        assert_eq!(d, get_vimvar_dict());
        assert_eq!(ht, unsafe { &mut (*d).dv_hashtab as *mut HashtabT });
        // "count" is a real, pre-populated entry (VV_COUNT isn't
        // VAR_UNKNOWN at construction time) - the hashtable lookup
        // must actually find it, not just return a usable-but-empty
        // dict.
        assert!(!hashitem_empty(unsafe { (*d).dv_hashtab.hash_find(b"count") }));
    }

    #[test]
    fn find_var_ht_dict_v_colon_val_and_key_are_not_pre_populated() {
        // v:val/v:key are VAR_UNKNOWN at construction time (only
        // populated transiently by prepare_vimvar/restore_vimvar
        // during map()/filter()/sort() closure evaluation, not yet
        // translated) - see VIMVARS's own doc comment.
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (ht, _varname, d) = find_var_ht_dict(b"v:val");
        assert_eq!(d, get_vimvar_dict());
        assert!(hashitem_empty(unsafe { (*ht).hash_find(b"val") }));
    }

    #[test]
    fn find_var_ht_dict_implicit_scope_finds_compat_flagged_vimvar() {
        // "version" is VV_COMPAT-flagged, so it must resolve via
        // implicit (no-scope-prefix) lookup straight to compat_hashtab,
        // without ever falling back to funccal-local/global scope.
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (ht, varname, d) = find_var_ht_dict(b"version");
        assert_eq!(varname, b"version");
        assert!(d.is_null());
        assert!(!ht.is_null());
        assert!(!hashitem_empty(unsafe { (*ht).hash_find(b"version") }));
    }

    #[test]
    fn garbage_collect_vimvars_marks_a_nested_dict_reachable_from_v() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let inner = crate::eval::typval::tv_dict_alloc();
        unsafe {
            (*inner).dv_copy_id = 0;
            set_vim_var_dict(VimVarIndex::Event, inner);
        }

        let aborted = unsafe { garbage_collect_vimvars(11) };

        assert!(!aborted);
        assert_eq!(unsafe { (*inner).dv_copy_id }, 11);

        // Clean up: detach v:event again and free the dict we made
        // (tv_dict_free ignores the refcount, matching this same test
        // module's own established cleanup precedent, e.g.
        // find_var_ht_dict_b_colon_resolves_to_curbuf_b_vars above) -
        // leaving VIMVARS/VIMVARDICT exactly as every other test
        // expects to find them.
        unsafe {
            set_vim_var_dict(VimVarIndex::Event, std::ptr::null_mut());
            crate::eval::typval::tv_dict_free(inner);
        }
    }

    #[test]
    fn find_var_ht_dict_a_colon_resolves_to_funccal_args_dict() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let mut fc = Box::new(FunccallT::default());
        fc.fc_l_vars.dv_refcount = DO_NOT_FREE_CNT; // gates get_funccal_args_dict too
        let fc_ptr = fc.as_mut() as *mut FunccallT;
        crate::eval::userfunc::set_current_funccal(fc_ptr);

        let (ht, varname, d) = find_var_ht_dict(b"a:1");
        assert_eq!(varname, b"1");
        assert_eq!(d, unsafe { &mut (*fc_ptr).fc_l_avars as *mut DictT });
        assert_eq!(ht, unsafe { &mut (*fc_ptr).fc_l_avars.dv_hashtab as *mut HashtabT });

        crate::eval::userfunc::set_current_funccal(std::ptr::null_mut());
    }

    #[test]
    fn find_var_ht_dict_l_colon_resolves_to_funccal_local_dict() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let mut fc = Box::new(FunccallT::default());
        fc.fc_l_vars.dv_refcount = DO_NOT_FREE_CNT;
        let fc_ptr = fc.as_mut() as *mut FunccallT;
        crate::eval::userfunc::set_current_funccal(fc_ptr);

        let (ht, varname, d) = find_var_ht_dict(b"l:foo");
        assert_eq!(varname, b"foo");
        assert_eq!(d, unsafe { &mut (*fc_ptr).fc_l_vars as *mut DictT });
        assert_eq!(ht, unsafe { &mut (*fc_ptr).fc_l_vars.dv_hashtab as *mut HashtabT });

        crate::eval::userfunc::set_current_funccal(std::ptr::null_mut());
    }

    #[test]
    fn find_var_ht_dict_s_colon_resolves_to_current_scripts_own_scope() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (sid, item) = crate::runtime::new_script_item(None);
        unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx.sc_sid = sid;
        let sv = unsafe { (*item).sn_vars };

        let (ht, varname, d) = find_var_ht_dict(b"s:foo");
        assert_eq!(varname, b"foo");
        assert_eq!(d, unsafe { &mut (*sv).sv_dict as *mut DictT });
        assert_eq!(ht, unsafe { &mut (*sv).sv_dict.dv_hashtab as *mut HashtabT });

        reset_shared_state();
    }

    #[test]
    fn find_var_ht_dict_s_colon_lazily_creates_a_script_item_for_sid_str() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx.sc_sid = crate::globals::SID_STR;

        let (ht, varname, d) = find_var_ht_dict(b"s:foo");
        assert_eq!(varname, b"foo");
        assert!(!d.is_null());
        assert!(!ht.is_null());
        // A brand-new, real script item was created and current_sctx
        // updated to point at it (no longer the SID_STR sentinel).
        let new_sid = unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx.sc_sid;
        assert!(new_sid > 0);
        assert_eq!(crate::runtime::script_item_count(), new_sid);

        reset_shared_state();
    }

    #[test]
    fn find_var_ht_dict_unknown_scope_letter_returns_null() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (ht, varname, d) = find_var_ht_dict(b"x:foo");
        assert_eq!(varname, b"foo");
        assert!(ht.is_null());
        assert!(d.is_null());
    }

    #[test]
    fn find_var_ht_delegates_to_find_var_ht_dict_dropping_the_dict() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (ht_from_dict, varname_from_dict, _d) = find_var_ht_dict(b"g:foo");
        let (ht, varname) = find_var_ht(b"g:foo");
        assert_eq!(ht, ht_from_dict);
        assert_eq!(varname, varname_from_dict);
        reset_shared_state();
    }

    // ---- find_var_in_ht: varname_len == 0 (whole-scope pseudo-item) ------

    #[test]
    fn find_var_in_ht_empty_varname_g_returns_globvars_var_pointing_at_globvardict() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let found = unsafe { find_var_in_ht(get_globvar_dict(), b'g', b"", false) };
        match found {
            Some(DictitemVariant::Scope(p)) => {
                assert_eq!(unsafe { &(*p).di_tv.value }, &TypvalValue::Dict(get_globvar_dict()));
            }
            other => panic!("expected Some(Scope(_)), got {other:?}"),
        }
    }

    #[test]
    fn find_var_in_ht_empty_varname_v_returns_vimvars_var_pointing_at_vimvardict() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let found = unsafe { find_var_in_ht(get_vimvar_dict(), b'v', b"", false) };
        match found {
            Some(DictitemVariant::Scope(p)) => {
                assert_eq!(unsafe { &(*p).di_tv.value }, &TypvalValue::Dict(get_vimvar_dict()));
            }
            other => panic!("expected Some(Scope(_)), got {other:?}"),
        }
    }

    #[test]
    fn find_var_in_ht_empty_varname_b_w_t_return_the_current_buf_win_tab_scope_vars() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let cbwt = TestCurBufWinTab::new();

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let expect_b = unsafe { &mut (*g.curbuf).b_bufvar as *mut ScopeDictDictItem };
        let expect_w = unsafe { &mut (*g.curwin).w_winvar as *mut ScopeDictDictItem };
        let expect_t = unsafe { &mut (*g.curtab).tp_winvar as *mut ScopeDictDictItem };

        assert_eq!(
            unsafe { find_var_in_ht(std::ptr::null_mut(), b'b', b"", false) },
            Some(DictitemVariant::Scope(expect_b))
        );
        assert_eq!(
            unsafe { find_var_in_ht(std::ptr::null_mut(), b'w', b"", false) },
            Some(DictitemVariant::Scope(expect_w))
        );
        assert_eq!(
            unsafe { find_var_in_ht(std::ptr::null_mut(), b't', b"", false) },
            Some(DictitemVariant::Scope(expect_t))
        );
        drop(cbwt);
    }

    #[test]
    fn find_var_in_ht_empty_varname_s_returns_the_current_scripts_own_sv_var() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (sid, item) = crate::runtime::new_script_item(None);
        unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx.sc_sid = sid;
        let sv = unsafe { (*item).sn_vars };
        let expect = unsafe { &mut (*sv).sv_var as *mut ScopeDictDictItem };

        assert_eq!(
            unsafe { find_var_in_ht(std::ptr::null_mut(), b's', b"", false) },
            Some(DictitemVariant::Scope(expect))
        );
        reset_shared_state();
    }

    #[test]
    fn find_var_in_ht_empty_varname_l_and_a_are_none_without_a_current_funccal() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        assert_eq!(unsafe { find_var_in_ht(std::ptr::null_mut(), b'l', b"", false) }, None);
        assert_eq!(unsafe { find_var_in_ht(std::ptr::null_mut(), b'a', b"", false) }, None);
    }

    #[test]
    fn find_var_in_ht_empty_varname_l_and_a_return_the_current_funccals_scope_vars() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let mut fc = Box::new(FunccallT::default());
        fc.fc_l_vars.dv_refcount = DO_NOT_FREE_CNT;
        let fc_ptr = fc.as_mut() as *mut FunccallT;
        crate::eval::userfunc::set_current_funccal(fc_ptr);

        let expect_l = unsafe { &mut (*fc_ptr).fc_l_vars_var as *mut ScopeDictDictItem };
        let expect_a = unsafe { &mut (*fc_ptr).fc_l_avars_var as *mut ScopeDictDictItem };
        assert_eq!(
            unsafe { find_var_in_ht(std::ptr::null_mut(), b'l', b"", false) },
            Some(DictitemVariant::Scope(expect_l))
        );
        assert_eq!(
            unsafe { find_var_in_ht(std::ptr::null_mut(), b'a', b"", false) },
            Some(DictitemVariant::Scope(expect_a))
        );

        crate::eval::userfunc::set_current_funccal(std::ptr::null_mut());
    }

    #[test]
    fn find_var_in_ht_empty_varname_unknown_letter_is_none() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        assert_eq!(unsafe { find_var_in_ht(std::ptr::null_mut(), b'x', b"", false) }, None);
    }

    // ---- find_var_in_ht: real hash lookup ---------------------------------

    #[test]
    fn find_var_in_ht_finds_an_existing_item_by_name() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let d = crate::eval::typval::tv_dict_alloc();
        unsafe { (*d).dv_refcount = DO_NOT_FREE_CNT };
        let item = crate::eval::typval::tv_dict_item_alloc(b"count");
        unsafe { (*item).di_tv.value = TypvalValue::Number(42) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *d, item) };

        let found = unsafe { find_var_in_ht(d, b'x', b"count", true) };
        assert_eq!(found, Some(DictitemVariant::Dict(item)));

        unsafe { crate::eval::typval::tv_dict_free(d) };
    }

    #[test]
    fn find_var_in_ht_missing_item_in_a_non_global_dict_is_none_without_autoload() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        // A fresh, non-global dict - script_autoload must never be
        // attempted here even with no_autoload=false, since the
        // original only ever tries it for the GLOBAL scope
        // specifically. If this were wrongly attempted, the
        // not-yet-translated substantive path would panic.
        let d = crate::eval::typval::tv_dict_alloc();
        unsafe { (*d).dv_refcount = DO_NOT_FREE_CNT };

        let found = unsafe { find_var_in_ht(d, b'x', b"missing#auto", false) };
        assert_eq!(found, None);

        unsafe { crate::eval::typval::tv_dict_free(d) };
    }

    #[test]
    fn find_var_in_ht_missing_plain_name_in_globvardict_is_none_without_reaching_autoload() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        // "missing" has no '#' at all, so script_autoload's own
        // fast-reject path answers false without ever reaching its
        // unimplemented!() branch - this must not panic.
        let found = unsafe { find_var_in_ht(get_globvar_dict(), b'g', b"missing", false) };
        assert_eq!(found, None);
    }

    #[test]
    fn find_var_in_ht_missing_autoload_style_name_in_globvardict_with_no_autoload_true_is_none() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        // no_autoload=true must skip the autoload attempt entirely,
        // even though d is globvardict and the name looks like a real
        // package name - must not panic.
        let found = unsafe { find_var_in_ht(get_globvar_dict(), b'g', b"Foo#bar", true) };
        assert_eq!(found, None);
    }

    #[test]
    #[should_panic]
    fn find_var_in_ht_missing_autoload_style_name_in_globvardict_reaches_unimplemented_autoload() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        // d IS globvardict, no_autoload=false, and the name genuinely
        // looks like a package reference ('#' not at position 0) -
        // this must reach script_autoload's real, substantive,
        // not-yet-translated (unimplemented!()) path.
        let _ = unsafe { find_var_in_ht(get_globvar_dict(), b'g', b"Foo#bar", false) };
    }

    // ---- find_var ----------------------------------------------------------

    #[test]
    fn find_var_empty_name_is_none_with_a_null_ht() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (item, ht) = unsafe { find_var(b"", false, false) };
        assert_eq!(item, None);
        assert!(ht.is_null());
    }

    #[test]
    fn find_var_finds_an_existing_global_variable() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let item = crate::eval::typval::tv_dict_item_alloc(b"count");
        unsafe { (*item).di_tv.value = TypvalValue::Number(7) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *get_globvar_dict(), item) };

        let (found, ht) = unsafe { find_var(b"g:count", false, false) };
        assert_eq!(found, Some(DictitemVariant::Dict(item)));
        assert_eq!(ht, unsafe { &mut (*get_globvar_dict()).dv_hashtab as *mut HashtabT });

        reset_shared_state();
    }

    #[test]
    fn find_var_missing_global_variable_is_none_but_still_returns_a_real_ht() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (found, ht) = unsafe { find_var(b"g:missing", false, false) };
        assert_eq!(found, None);
        assert!(!ht.is_null());
    }

    #[test]
    fn find_var_want_ht_true_forces_no_autoload_and_avoids_the_unimplemented_panic() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        // Without want_ht forcing no_autoload on, this exact lookup
        // would reach script_autoload's unimplemented!() path (proven
        // by the sibling #[should_panic] test below) - want_ht=true
        // must prevent that.
        let (found, ht) = unsafe { find_var(b"g:Foo#bar", true, false) };
        assert_eq!(found, None);
        assert!(!ht.is_null());
    }

    #[test]
    #[should_panic]
    fn find_var_without_want_ht_reaches_the_unimplemented_autoload_path() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let _ = unsafe { find_var(b"g:Foo#bar", false, false) };
    }

    #[test]
    fn find_var_falls_back_to_the_enclosing_lambda_scope_when_not_found_locally() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        // outer_fc: the enclosing function's own funccall, holding the
        // real "x" variable in its l: scope. All field access after
        // deriving outer_fc_ptr goes through THAT raw pointer, not
        // through outer_fc directly - re-borrowing through the Box
        // after a raw pointer has already been derived from it is a
        // real Stacked Borrows violation (confirmed by Miri) that
        // invalidates the earlier pointer, even though the underlying
        // memory address doesn't change.
        let mut outer_fp = crate::eval::typval_defs::UfuncT::default();
        let mut outer_fc = Box::new(FunccallT::default());
        outer_fc.fc_l_vars.dv_refcount = DO_NOT_FREE_CNT;
        outer_fc.fc_func = &mut outer_fp as *mut _;
        let outer_fc_ptr = outer_fc.as_mut() as *mut FunccallT;
        let item = crate::eval::typval::tv_dict_item_alloc(b"x");
        unsafe { (*item).di_tv.value = TypvalValue::Number(99) };
        unsafe { crate::eval::typval::tv_dict_add(&mut (*outer_fc_ptr).fc_l_vars, item) };

        // inner_fp/inner_fc: the currently-executing lambda, scoped to
        // outer_fc via uf_scoped - its OWN l: scope does not have "x".
        let mut inner_fp = crate::eval::typval_defs::UfuncT { uf_scoped: outer_fc_ptr, ..Default::default() };
        let mut inner_fc = Box::new(FunccallT::default());
        inner_fc.fc_l_vars.dv_refcount = DO_NOT_FREE_CNT;
        inner_fc.fc_func = &mut inner_fp as *mut _;
        let inner_fc_ptr = inner_fc.as_mut() as *mut FunccallT;
        crate::eval::userfunc::set_current_funccal(inner_fc_ptr);

        let (found, _ht) = unsafe { find_var(b"x", false, false) };
        assert_eq!(found, Some(DictitemVariant::Dict(item)));

        // CURRENT_FUNCCAL must be restored to the inner (originally
        // current) funccal, not left pointing at the outer one.
        assert_eq!(crate::eval::userfunc::get_current_funccal(), inner_fc_ptr);

        crate::eval::userfunc::set_current_funccal(std::ptr::null_mut());
    }

    #[test]
    fn find_var_returns_none_when_not_found_locally_or_in_any_enclosing_scope() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        let mut outer_fp = crate::eval::typval_defs::UfuncT::default();
        let mut outer_fc = Box::new(FunccallT::default());
        outer_fc.fc_l_vars.dv_refcount = DO_NOT_FREE_CNT;
        outer_fc.fc_func = &mut outer_fp as *mut _;
        let outer_fc_ptr = outer_fc.as_mut() as *mut FunccallT;

        let mut inner_fp = crate::eval::typval_defs::UfuncT { uf_scoped: outer_fc_ptr, ..Default::default() };
        let mut inner_fc = Box::new(FunccallT::default());
        inner_fc.fc_l_vars.dv_refcount = DO_NOT_FREE_CNT;
        inner_fc.fc_func = &mut inner_fp as *mut _;
        let inner_fc_ptr = inner_fc.as_mut() as *mut FunccallT;
        crate::eval::userfunc::set_current_funccal(inner_fc_ptr);

        let (found, _ht) = unsafe { find_var(b"nowhere", false, false) };
        assert_eq!(found, None);
        assert_eq!(crate::eval::userfunc::get_current_funccal(), inner_fc_ptr);

        crate::eval::userfunc::set_current_funccal(std::ptr::null_mut());
    }

    // ---- eval_variable ----

    #[test]
    fn eval_variable_undefined_returns_fail() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        let mut rettv = TypvalT::default();
        let ret = unsafe { eval_variable(b"g:nope", Some(&mut rettv), false, false) };
        assert_eq!(ret, crate::vim_defs::FAIL);

        reset_shared_state();
    }

    #[test]
    fn eval_variable_found_copies_the_value_into_rettv() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        let item = crate::eval::typval::tv_dict_item_alloc(b"count");
        unsafe { (*item).di_tv.value = TypvalValue::Number(7) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *get_globvar_dict(), item) };

        let mut rettv = TypvalT::default();
        let ret = unsafe { eval_variable(b"g:count", Some(&mut rettv), true, false) };
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(rettv.value, TypvalValue::Number(7));

        reset_shared_state();
    }

    #[test]
    fn eval_variable_with_no_rettv_still_reports_found_or_not() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        assert_eq!(unsafe { eval_variable(b"g:nope", None, false, false) }, crate::vim_defs::FAIL);

        let item = crate::eval::typval::tv_dict_item_alloc(b"count");
        unsafe { (*item).di_tv.value = TypvalValue::Number(1) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *get_globvar_dict(), item) };
        assert_eq!(unsafe { eval_variable(b"g:count", None, false, false) }, crate::vim_defs::OK);

        reset_shared_state();
    }

    #[test]
    fn eval_variable_reads_whole_scope_dict_for_a_bare_g_reference() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        let mut rettv = TypvalT::default();
        let ret = unsafe { eval_variable(b"g:", Some(&mut rettv), true, false) };
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(rettv.value, TypvalValue::Dict(get_globvar_dict()));

        reset_shared_state();
    }

    // ---- var_exists ----

    #[test]
    fn var_exists_true_for_a_defined_global_variable() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        let item = crate::eval::typval::tv_dict_item_alloc(b"count");
        unsafe { (*item).di_tv.value = TypvalValue::Number(7) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *get_globvar_dict(), item) };

        assert!(unsafe { var_exists(b"g:count") });

        reset_shared_state();
    }

    #[test]
    fn var_exists_false_for_an_undefined_variable() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        assert!(!unsafe { var_exists(b"g:definitely_not_defined") });

        reset_shared_state();
    }

    #[test]
    fn var_exists_false_for_trailing_garbage_after_the_name() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        let item = crate::eval::typval::tv_dict_item_alloc(b"count");
        unsafe { (*item).di_tv.value = TypvalValue::Number(7) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *get_globvar_dict(), item) };

        // A subscript chain after the name (` .foo`) is trailing
        // garbage for a Number value (`.` only continues for a
        // Dict, per handle_subscript's own doc) - handle_subscript's
        // fast path correctly reports nothing consumed, and the
        // final "anything left over" check makes this false rather
        // than panicking.
        assert!(!unsafe { var_exists(b"g:count extra") });

        reset_shared_state();
    }

    #[test]
    fn var_exists_true_for_a_defined_option() {
        // Not var_exists itself (options are a completely different
        // f_exists() branch, "&"/"+" prefixed) - included here only
        // to double check get_name_len/get_id_len don't misparse an
        // ordinary bare word used as a plain (undefined) variable
        // name that happens to also be a real option name.
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        assert!(!unsafe { var_exists(b"number") });

        reset_shared_state();
    }

    #[test]
    fn var_exists_false_for_an_empty_string() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        assert!(!unsafe { var_exists(b"") });

        reset_shared_state();
    }

    // ---- check_vars ----

    #[test]
    fn check_vars_is_a_no_op_when_eval_lavars_used_is_none() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        assert!(unsafe { *EVAL_LAVARS_USED.get_mut() }.is_none());

        // Must not panic even for a name that would otherwise resolve.
        unsafe { check_vars(b"g:anything") };
    }

    #[test]
    fn check_vars_sets_the_flag_for_a_local_funccal_variable() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        let mut fp = crate::eval::typval_defs::UfuncT::default();
        let mut fc = Box::new(FunccallT::default());
        fc.fc_l_vars.dv_refcount = DO_NOT_FREE_CNT;
        fc.fc_func = &mut fp as *mut _;
        let item = crate::eval::typval::tv_dict_item_alloc(b"x");
        unsafe { (*item).di_tv.value = TypvalValue::Number(1) };
        unsafe { crate::eval::typval::tv_dict_add(&mut fc.fc_l_vars, item) };
        crate::eval::userfunc::set_current_funccal(fc.as_mut() as *mut FunccallT);

        let mut used = false;
        unsafe { *EVAL_LAVARS_USED.get_mut() = Some(&mut used as *mut bool) };
        unsafe { check_vars(b"x") };
        assert!(used, "expected check_vars to set the local-var-used flag");

        unsafe { *EVAL_LAVARS_USED.get_mut() = None };
        crate::eval::userfunc::set_current_funccal(std::ptr::null_mut());
        reset_shared_state();
    }

    #[test]
    fn check_vars_does_not_set_the_flag_for_a_global_variable() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        let item = crate::eval::typval::tv_dict_item_alloc(b"g_only");
        unsafe { (*item).di_tv.value = TypvalValue::Number(1) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *get_globvar_dict(), item) };

        let mut used = false;
        unsafe { *EVAL_LAVARS_USED.get_mut() = Some(&mut used as *mut bool) };
        unsafe { check_vars(b"g:g_only") };
        assert!(!used, "a global variable is not a local var/arg");

        unsafe { *EVAL_LAVARS_USED.get_mut() = None };
        reset_shared_state();
    }
}

#[cfg(test)]
mod optval_as_tv_tests {
    use super::*;
    use crate::option_defs::OptVal;
    use crate::types_defs::TriState;

    #[test]
    fn nil_becomes_special_null() {
        let tv = optval_as_tv(OptVal::Nil, true);
        assert_eq!(tv.value, TypvalValue::Special(SpecialVarValue::Null));
        assert_eq!(tv.v_lock, VarLockStatus::Unlocked);
    }

    #[test]
    fn boolean_true_with_numbool_becomes_number_one() {
        let tv = optval_as_tv(OptVal::Boolean(TriState::True), true);
        assert_eq!(tv.value, TypvalValue::Number(1));
    }

    #[test]
    fn boolean_false_with_numbool_becomes_number_zero() {
        let tv = optval_as_tv(OptVal::Boolean(TriState::False), true);
        assert_eq!(tv.value, TypvalValue::Number(0));
    }

    #[test]
    fn boolean_none_with_numbool_becomes_number_negative_one() {
        // Even TriState::None becomes the NUMBER -1 (not null!) when
        // numbool is true - matching the original's own
        // `tv->vval.v_number = (varnumber_T)value.data.boolean;`
        // unconditional cast, with TriState::kNone == -1.
        let tv = optval_as_tv(OptVal::Boolean(TriState::None), true);
        assert_eq!(tv.value, TypvalValue::Number(-1));
    }

    #[test]
    fn boolean_true_without_numbool_becomes_bool_true() {
        let tv = optval_as_tv(OptVal::Boolean(TriState::True), false);
        assert_eq!(tv.value, TypvalValue::Bool(BoolVarValue::True));
    }

    #[test]
    fn boolean_false_without_numbool_becomes_bool_false() {
        let tv = optval_as_tv(OptVal::Boolean(TriState::False), false);
        assert_eq!(tv.value, TypvalValue::Bool(BoolVarValue::False));
    }

    #[test]
    fn boolean_none_without_numbool_becomes_special_null() {
        // Neither the "numbool" branch nor the "b != None" branch
        // fires - stays at the function's initial VAR_SPECIAL/
        // kSpecialVarNull default in the original.
        let tv = optval_as_tv(OptVal::Boolean(TriState::None), false);
        assert_eq!(tv.value, TypvalValue::Special(SpecialVarValue::Null));
    }

    #[test]
    fn number_becomes_number_directly() {
        let tv = optval_as_tv(OptVal::Number(42), true);
        assert_eq!(tv.value, TypvalValue::Number(42));
        let tv = optval_as_tv(OptVal::Number(-7), false);
        assert_eq!(tv.value, TypvalValue::Number(-7));
    }

    #[test]
    fn string_becomes_string_directly() {
        let tv = optval_as_tv(OptVal::String(b"hello".to_vec()), true);
        assert_eq!(tv.value, TypvalValue::String(Some(b"hello".to_vec())));
    }

    #[test]
    fn empty_string_becomes_some_empty_vec_not_none() {
        // NvimString = Vec<u8>, always present, never null in this
        // Rust representation.
        let tv = optval_as_tv(OptVal::String(Vec::new()), true);
        assert_eq!(tv.value, TypvalValue::String(Some(Vec::new())));
    }
}

#[cfg(test)]
mod eval_one_expr_in_str_tests {
    use super::*;

    #[test]
    fn simple_number_expression() {
        let mut gap = Vec::new();
        let consumed = unsafe { eval_one_expr_in_str(b"{42}rest", &mut gap, true) };
        assert_eq!(consumed, Some(4));
        assert_eq!(gap, b"42");
    }

    #[test]
    fn whitespace_around_the_expression_is_skipped() {
        let mut gap = Vec::new();
        let consumed = unsafe { eval_one_expr_in_str(b"{ 42 }", &mut gap, true) };
        assert_eq!(consumed, Some(6));
        assert_eq!(gap, b"42");
    }

    #[test]
    fn string_concatenation_expression() {
        let mut gap = b"prefix-".to_vec();
        let consumed = unsafe { eval_one_expr_in_str(b"{1 + 1}", &mut gap, true) };
        assert_eq!(consumed, Some(7));
        assert_eq!(gap, b"prefix-2");
    }

    #[test]
    fn missing_closing_curly_fails() {
        let mut gap = Vec::new();
        let consumed = unsafe { eval_one_expr_in_str(b"{42", &mut gap, true) };
        assert_eq!(consumed, None);
        assert!(gap.is_empty());
    }

    #[test]
    fn invalid_expression_fails() {
        let mut gap = Vec::new();
        let consumed = unsafe { eval_one_expr_in_str(b"{}", &mut gap, true) };
        assert_eq!(consumed, None);
    }

    #[test]
    fn evaluate_false_still_validates_but_does_not_append() {
        let mut gap = Vec::new();
        let consumed = unsafe { eval_one_expr_in_str(b"{42}rest", &mut gap, false) };
        assert_eq!(consumed, Some(4));
        assert!(gap.is_empty(), "evaluate=false must not append anything to gap");
    }

    #[test]
    fn evaluate_false_still_fails_on_missing_closing_curly() {
        let mut gap = Vec::new();
        let consumed = unsafe { eval_one_expr_in_str(b"{42", &mut gap, false) };
        assert_eq!(consumed, None);
    }
}

/// Implements the logic to retrieve local variable and option values.
/// Used by `getwinvar()`/`gettabvar()`/`gettabwinvar()`/`getbufvar()`
/// (`get_var_from`).
///
/// `varname` is `None` for a caller whose own `tv_get_string_chk` call
/// already failed (matching the original's own nullable `const char
/// *varname`). `tp`/`win`/`buf` may each be null, matching the
/// original's own nullable pointers - `buf` is ignored unless
/// `htname == b'b'`.
///
/// # Safety
/// `tp`/`win`/`buf` (whichever are non-null) must be valid, live
/// pointers; `GLOBALS.curtab`/`curwin`/`curbuf` must be valid.
///
/// # Panics
/// Panics if a real window/tabpage switch away from the current one
/// is genuinely needed (`tp != curtab` or `win != curwin`, whenever
/// `do_change_curbuf` doesn't already sidestep it for `htname ==
/// b'b'`) - needs the real `ctx_switch` (`context.c`), which itself
/// needs window/tabpage-switching machinery (`goto_tabpage_tp`,
/// `use_tabpage`/`unuse_tabpage`) not yet translated. Not reachable
/// via `getbufvar({buf}, ...)` (any `{buf}`, since `do_change_curbuf`
/// always sidesteps the switch for `htname == b'b'`) or
/// `getwinvar(0, ...)`/`gettabvar(tabnr-of-curtab, ...)` (the current
/// window/tab, the overwhelmingly common invocation shape). The bare
/// `"&"` (whole window/buffer-local-options-dict) form is now real
/// too, via `get_winbuf_options` - no longer a panic case.
#[allow(clippy::too_many_arguments)]
unsafe fn get_var_from(
    varname: Option<&[u8]>,
    rettv: &mut TypvalT,
    deftv: &TypvalT,
    htname: u8,
    tp: *mut crate::buffer_defs::TabpageT,
    win: *mut crate::buffer_defs::WinT,
    buf: *mut crate::buffer_defs::BufT,
) {
    let mut done = false;
    let do_change_curbuf = !buf.is_null() && htname == b'b';

    rettv.value = TypvalValue::String(None);

    if let Some(varname) = varname {
        if !tp.is_null() && !win.is_null() && (htname != b'b' || !buf.is_null()) {
            // SAFETY: forwarded from this function's own safety doc.
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let need_switch_win =
                !(std::ptr::eq(tp, g.curtab) && std::ptr::eq(win, g.curwin)) && !do_change_curbuf;

            if need_switch_win {
                unimplemented!(
                    "get_var_from: switching to a different window/tabpage needs the real \
                     ctx_switch (context.c), not yet translated - see this function's own doc \
                     comment"
                );
            }

            if varname.first() == Some(&b'&') && htname != b't' {
                // SAFETY: forwarded from this function's own safety doc.
                let save_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
                if do_change_curbuf {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = buf;
                }

                if varname.len() == 1 {
                    // get all window-local or buffer-local options in
                    // a dict.
                    // SAFETY: forwarded from this function's own
                    // safety doc - GLOBALS.curbuf/curwin were just
                    // set up above (curbuf possibly swapped to `buf`
                    // for the `do_change_curbuf` case).
                    let opts = unsafe { crate::option::get_winbuf_options(htname == b'b') };
                    // `get_winbuf_options` always returns a valid,
                    // non-null `Box::into_raw` pointer in this crate
                    // (Rust's allocator aborts rather than returning
                    // null on failure, unlike the original's own
                    // malloc-can-fail check) - the null guard is kept
                    // anyway, faithfully mirroring the original's own
                    // structure.
                    if !opts.is_null() {
                        // SAFETY: forwarded from this function's own
                        // safety doc.
                        unsafe { crate::eval::typval::tv_dict_set_ret(rettv, opts) };
                        done = true;
                    }
                } else {
                    // SAFETY: forwarded from this function's own safety doc.
                    let (r, _) =
                        unsafe { crate::eval::eval::eval_option(varname, Some(rettv), true) };
                    if r == crate::vim_defs::OK {
                        done = true;
                    }
                }

                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = save_curbuf;
            } else if varname.is_empty() {
                let v: *const ScopeDictDictItem = if htname == b'b' {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { &(*buf).b_bufvar }
                } else if htname == b'w' {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { &(*win).w_winvar }
                } else {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { &(*tp).tp_winvar }
                };
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::eval::typval::tv_copy(&(*v).di_tv, rettv) };
                done = true;
            } else {
                let d: *mut DictT = if htname == b'b' {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { (*buf).b_vars }
                } else if htname == b'w' {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { (*win).w_vars }
                } else {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { (*tp).tp_vars }
                };
                // SAFETY: forwarded from this function's own safety doc.
                let found = unsafe { find_var_in_ht(d, htname, varname, false) };
                if let Some(variant) = found {
                    let src: *const TypvalT = match variant {
                        // SAFETY: forwarded from this function's own safety doc.
                        DictitemVariant::Dict(p) => unsafe { &(*p).di_tv },
                        // SAFETY: forwarded from this function's own safety doc.
                        DictitemVariant::Scope(p) => unsafe { &(*p).di_tv },
                    };
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { crate::eval::typval::tv_copy(&*src, rettv) };
                    done = true;
                }
            }
        }
    }

    if !done && !matches!(deftv.value, TypvalValue::Unknown) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::typval::tv_copy(deftv, rettv) };
    }
}

/// `getwinvar()`/`gettabwinvar()` (`getwinvar`).
///
/// `off == 1` selects `gettabwinvar()`'s extra leading `{tabnr}`
/// argument; `off == 0` is plain `getwinvar()`.
///
/// # Safety
/// Forwarded from `get_var_from`'s own safety doc.
unsafe fn getwinvar(argvars: &[TypvalT], rettv: &mut TypvalT, off: usize) {
    let tp = if off == 1 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            crate::window::find_tabpage(
                crate::eval::typval::tv_get_number_chk(&argvars[0], None) as i32,
            )
        }
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab
    };
    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { crate::window::find_win_by_nr(&argvars[off], tp) };
    let varname = crate::eval::typval::tv_get_string_chk(&argvars[off + 1]);

    // The optional trailing {def} argument may simply be absent from
    // this crate's own exactly-sized argvars slice (unlike the
    // original's fixed-size, VAR_UNKNOWN-padded array) - fall back to
    // a plain default (VAR_UNKNOWN) TypvalT in that case, matching
    // get_var_from's own "deftv.v_type == VAR_UNKNOWN" no-default
    // check.
    let default_tv = TypvalT::default();
    let deftv = argvars.get(off + 2).unwrap_or(&default_tv);

    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        get_var_from(varname.as_deref(), rettv, deftv, b'w', tp, win, std::ptr::null_mut());
    }
}

/// `"gettabvar()"` function (`f_gettabvar`).
///
/// # Safety
/// Forwarded from `get_var_from`'s own safety doc.
pub unsafe fn f_gettabvar(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let varname = crate::eval::typval::tv_get_string_chk(&argvars[1]);
    // SAFETY: forwarded from this function's own safety doc.
    let tp = unsafe {
        crate::window::find_tabpage(crate::eval::typval::tv_get_number_chk(&argvars[0], None) as i32)
    };
    let win = if tp.is_null() {
        std::ptr::null_mut()
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        // SAFETY: forwarded from this function's own safety doc; tp
        // was just null-checked above.
        if std::ptr::eq(tp, g.curtab) || unsafe { &*tp }.tp_firstwin.is_null() {
            g.firstwin
        } else {
            // SAFETY: forwarded from this function's own safety doc;
            // tp was just null-checked above.
            unsafe { &*tp }.tp_firstwin
        }
    };

    let default_tv = TypvalT::default();
    let deftv = argvars.get(2).unwrap_or(&default_tv);
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        get_var_from(varname.as_deref(), rettv, deftv, b't', tp, win, std::ptr::null_mut());
    }
}

/// `"gettabwinvar()"` function (`f_gettabwinvar`).
///
/// # Safety
/// Forwarded from `get_var_from`'s own safety doc.
pub unsafe fn f_gettabwinvar(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { getwinvar(argvars, rettv, 1) };
}

/// `"getwinvar()"` function (`f_getwinvar`).
///
/// # Safety
/// Forwarded from `get_var_from`'s own safety doc.
pub unsafe fn f_getwinvar(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { getwinvar(argvars, rettv, 0) };
}

/// `"getbufvar()"` function (`f_getbufvar`).
///
/// # Safety
/// Forwarded from `get_var_from`'s own safety doc.
pub unsafe fn f_getbufvar(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let varname = crate::eval::typval::tv_get_string_chk(&argvars[1]);
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { crate::eval::buffer::tv_get_buf_from_arg(&argvars[0]) };
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let default_tv = TypvalT::default();
    let deftv = argvars.get(2).unwrap_or(&default_tv);
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        get_var_from(varname.as_deref(), rettv, deftv, b'b', g.curtab, g.curwin, buf);
    }
}

/// Set variable `name` to the value of `tv` (`set_var`). The caller
/// decides whether to copy the value (`copy = true`) or move it
/// (`copy = false` - `*tv` becomes `Unknown` afterward, matching the
/// original's own `tv_init(tv)`).
///
/// # Safety
/// Forwarded from [`set_var_const`]'s own safety doc.
pub unsafe fn set_var(name: &[u8], tv: &mut TypvalT, copy: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_var_const(name, tv, copy, false) };
}

/// [`set_var`] with support for `:const`'s `is_const` (`set_var_const`).
///
/// # Safety
/// `name` must resolve (via [`find_var_ht_dict`]) to a dict/hashtab
/// this crate's own global state (`GLOBALS`/`VIMVARDICT`/
/// `GLOBVARDICT`/a live funccall's own scope dicts) can actually
/// provide - i.e. the same safety contract [`find_var_ht_dict`]/
/// [`find_var_in_ht`] themselves already carry.
///
/// # Panics
/// Panics if `tv` holds a `Func`/`Partial` value - needs
/// `var_wrong_func_name` -> `function_exists` ->
/// `trans_function_name`, not yet translated. Also panics if `name`
/// resolves into the `v:` scope dict specifically - needs
/// `before_set_vvar`, not yet translated. Neither is reached by
/// `settabvar`/`setwinvar`/`setbufvar` (which only ever target
/// `t:`/`w:`/`b:` names) or by setting an ordinary Number/Float/
/// String/List/Dict/Blob value.
pub unsafe fn set_var_const(name: &[u8], tv: &mut TypvalT, copy: bool, is_const: bool) {
    let (ht, varname, dict) = find_var_ht_dict(name);
    // `watched` is always false: DictT has no `watchers` field at all
    // yet (needs a QUEUE intrusive-linked-list translation first,
    // matching this crate's own already-documented gap for DictT
    // itself) - tv_dict_is_watched's real contract
    // (`d && !QUEUE_EMPTY(&d->watchers)`) can never be true for any
    // dict this crate can construct today.

    if ht.is_null() || varname.is_empty() {
        // semsg(_(e_illvar), name) omitted (message display, not
        // tractable) - the identical early return is kept.
        return;
    }

    if crate::eval::typval::tv_is_func(tv) {
        unimplemented!(
            "set_var_const: setting a Func/Partial value needs var_wrong_func_name -> \
             function_exists -> trans_function_name, not yet translated - see this function's \
             own doc comment"
        );
    }

    // SAFETY: forwarded from this function's own safety doc; dict is
    // non-null since ht is non-null (find_var_ht_dict's own contract).
    let mut found = unsafe { find_var_in_ht(dict, 0, varname, true) };
    if found.is_none() {
        // Search in parent scope which is possible to reference from
        // a lambda.
        // SAFETY: forwarded from this function's own safety doc.
        found = unsafe { crate::eval::userfunc::find_var_in_scoped_ht(name, true) };
    }

    let di_tv: *mut TypvalT;

    match found {
        Some(variant) => {
            if is_const {
                // emsg(_(e_cannot_mod)) omitted (message display).
                return;
            }
            let (di_flags, tv_ptr): (*mut u8, *mut TypvalT) = match variant {
                DictitemVariant::Dict(p) => {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { (&mut (*p).di_flags as *mut u8, &mut (*p).di_tv as *mut TypvalT) }
                }
                DictitemVariant::Scope(p) => {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { (&mut (*p).di_flags as *mut u8, &mut (*p).di_tv as *mut TypvalT) }
                }
            };
            // Check in this order for backwards compatibility: whether
            // the variable is read-only, whether the variable VALUE is
            // locked, whether the variable itself is locked.
            // SAFETY: forwarded from this function's own safety doc.
            let flags = unsafe { *di_flags };
            // SAFETY: forwarded from this function's own safety doc.
            let lock = unsafe { (*tv_ptr).v_lock };
            if var_check_ro(flags) || value_check_lock(lock, None) || var_check_lock(flags) {
                return;
            }
            // existing variable, need to clear the value.
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_clear_simple(&*tv_ptr) };
            di_tv = tv_ptr;
        }
        None => {
            // Can't add "v:" or "a:" variable.
            if std::ptr::eq(dict, get_vimvar_dict())
                || std::ptr::eq(dict, crate::eval::userfunc::get_funccal_args_dict())
            {
                // semsg(_(e_illvar), name) omitted.
                return;
            }

            if !valid_varname(varname) {
                return;
            }

            let item = crate::eval::typval::tv_dict_item_alloc(varname);
            // SAFETY: dict is non-null, forwarded from this function's
            // own safety doc.
            if unsafe { crate::eval::typval::tv_dict_add(&mut *dict, item) } == crate::vim_defs::FAIL
            {
                // SAFETY: item was just allocated via
                // tv_dict_item_alloc's own Box::into_raw, never added
                // to any hashtable (the add itself just failed).
                drop(unsafe { Box::from_raw(item) });
                return;
            }
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { (*item).di_flags = dict_item_flags::ALLOC };
            if is_const {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { (*item).di_flags |= dict_item_flags::LOCK };
            }
            // SAFETY: forwarded from this function's own safety doc.
            di_tv = unsafe { &mut (*item).di_tv };
        }
    }

    if copy || matches!(tv.value, TypvalValue::Number(_) | TypvalValue::Float(_)) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::typval::tv_copy(tv, &mut *di_tv) };
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { *di_tv = std::mem::take(tv) };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*di_tv).v_lock = VarLockStatus::Unlocked };
        // tv_init(tv) - already done by mem::take above, which leaves
        // *tv at its Default (Unknown), matching tv_init's own
        // memset-to-zero contract.
    }

    if is_const {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            crate::eval::typval::tv_item_lock(
                &mut *di_tv,
                crate::eval::typval::DICT_MAXNEST,
                true,
                true,
            );
        }
    }
}

/// Set an internal variable to a string value, creating it if it
/// doesn't already exist (`set_internal_string_var`). `value` models
/// the original's own real (if unusually not `FUNC_ATTR_NONNULL`-
/// asserted for this parameter) nullable `char *value` - several real
/// callers (`ex_cmds2.c`'s `:compiler`, `syntax.c`'s `'syntax'`
/// bookkeeping) can genuinely pass a `NULL` "no previous value" case.
///
/// None of this function's real callers (`ex_cmds2.c`, `quickfix.c`,
/// `statusline.c`, `syntax.c`) are translated yet - harvested ahead of
/// them, matching this crate's established precedent for a small,
/// self-contained function with no design freedom of its own (a thin
/// `TypvalT` + [`set_var`] wrapper).
///
/// # Safety
/// Forwarded from [`set_var`]'s own safety doc.
pub unsafe fn set_internal_string_var(name: &[u8], value: Option<&[u8]>) {
    let mut tv = TypvalT { value: TypvalValue::String(value.map(<[u8]>::to_vec)), ..Default::default() };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_var(name, &mut tv, true) };
}

/// `"setwinvar()"`/`"settabwinvar()"` functions (`setwinvar`).
///
/// `off == 1` selects `settabwinvar()`'s extra leading `{tabnr}`
/// argument; `off == 0` is plain `setwinvar()`.
///
/// # Panics
/// Panics if a real window/tabpage switch away from the current one
/// is genuinely needed - needs the real `ctx_switch` (`context.c`),
/// not yet translated (see [`get_var_from`]'s own doc comment for the
/// identical, already-established reasoning). Also panics if
/// `{varname}` starts with `&` (setting an OPTION rather than a
/// plain `w:` variable) - needs `set_option_from_tv` ->
/// `set_option_value` (the generic option-value WRITE engine), not
/// yet translated. Neither is reached by `setwinvar(0, "name", ...)`/
/// `settabwinvar(<current tab>, 0, "name", ...)` (the current
/// window, the overwhelmingly common invocation shape) setting an
/// ordinary (non-`&`-prefixed) variable name.
///
/// # Safety
/// Forwarded from [`get_var_from`]'s own safety doc.
unsafe fn setwinvar(argvars: &[TypvalT], off: usize) {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::ex_cmds::check_secure() } {
        return;
    }

    let tp = if off == 1 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            crate::window::find_tabpage(
                crate::eval::typval::tv_get_number_chk(&argvars[0], None) as i32,
            )
        }
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab
    };
    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { crate::window::find_win_by_nr(&argvars[off], tp) };
    let varname = crate::eval::typval::tv_get_string_chk(&argvars[off + 1]);

    if win.is_null() || varname.is_none() {
        return;
    }
    let varname = varname.unwrap();

    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let need_switch_win = !(std::ptr::eq(tp, g.curtab) && std::ptr::eq(win, g.curwin));
    if need_switch_win {
        unimplemented!(
            "setwinvar: switching to a different window/tabpage needs the real ctx_switch \
             (context.c), not yet translated - see this function's own doc comment"
        );
    }

    if varname.first() == Some(&b'&') {
        unimplemented!(
            "setwinvar: setting an option (\"&name\") needs set_option_from_tv -> \
             set_option_value, not yet translated - see this function's own doc comment"
        );
    }

    let mut winvarname = Vec::with_capacity(varname.len() + 2);
    winvarname.extend_from_slice(b"w:");
    winvarname.extend_from_slice(&varname);
    // SAFETY: forwarded from this function's own safety doc; argvars
    // is only ever read through &argvars[off + 2] here, not mutated
    // (copy = true, matching the original's own set_var(..., true)).
    let mut varp = argvars[off + 2].clone();
    unsafe { set_var(&winvarname, &mut varp, true) };
}

/// `"settabwinvar()"` function (`f_settabwinvar`).
///
/// # Safety
/// Forwarded from `setwinvar`'s own safety doc.
pub unsafe fn f_settabwinvar(argvars: &[TypvalT], _rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { setwinvar(argvars, 1) };
}

/// `"setwinvar()"` function (`f_setwinvar`).
///
/// # Safety
/// Forwarded from `setwinvar`'s own safety doc.
pub unsafe fn f_setwinvar(argvars: &[TypvalT], _rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { setwinvar(argvars, 0) };
}

/// `"setbufvar()"` function (`f_setbufvar`).
///
/// # Panics
/// Panics if `{varname}` starts with `&` (setting an OPTION rather
/// than a plain `b:` variable) - needs the real `ctx_switch`
/// (`context.c`, called here UNCONDITIONALLY even for `{buf} ==
/// curbuf`, unlike `get_var_from`'s own narrower "only when
/// genuinely switching buffers" need) plus `set_option_from_tv` ->
/// `set_option_value`, neither yet translated. Not reached by setting
/// an ordinary (non-`&`-prefixed) variable name, on any buffer.
///
/// # Safety
/// `{buf}`, once resolved via `crate::eval::buffer::tv_get_buf_from_arg`,
/// must be a valid, live `BufT` pointer (or null) - forwarded from
/// that function's own safety doc.
pub unsafe fn f_setbufvar(argvars: &[TypvalT], _rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::ex_cmds::check_secure() }
        || !crate::eval::typval::tv_check_str_or_nr(&argvars[0])
    {
        return;
    }
    let varname = crate::eval::typval::tv_get_string_chk(&argvars[1]);
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { crate::eval::buffer::tv_get_buf(&argvars[0]) };

    if buf.is_null() || varname.is_none() {
        return;
    }
    let varname = varname.unwrap();

    if varname.first() == Some(&b'&') {
        unimplemented!(
            "f_setbufvar: setting an option (\"&name\") needs the real ctx_switch \
             (context.c) + set_option_from_tv -> set_option_value, not yet translated - see \
             this function's own doc comment"
        );
    }

    let mut bufvarname = Vec::with_capacity(varname.len() + 2);
    bufvarname.extend_from_slice(b"b:");
    bufvarname.extend_from_slice(&varname);
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let save_curbuf = g.curbuf;
    g.curbuf = buf;
    let mut varp = argvars[2].clone();
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_var(&bufvarname, &mut varp, true) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = save_curbuf;
}

#[cfg(test)]
mod get_var_from_tests {
    use super::*;

    fn reset_shared_state() {
        crate::eval::userfunc::set_current_funccal(std::ptr::null_mut());
        unsafe { vars_clear(GLOBVARDICT.get_mut()) };
        crate::runtime::tests_reset_for_test();
        unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx = Default::default();
    }

    pub(super) struct TestFixture {
        buf: Box<crate::buffer_defs::BufT>,
        win: Box<crate::buffer_defs::WinT>,
        tab: Box<crate::buffer_defs::TabpageT>,
        prev_curbuf: *mut crate::buffer_defs::BufT,
        prev_curwin: *mut crate::buffer_defs::WinT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
        prev_firstwin: *mut crate::buffer_defs::WinT,
        prev_first_tabpage: *mut crate::buffer_defs::TabpageT,
        prev_lastbuf: *mut crate::buffer_defs::BufT,
    }

    impl TestFixture {
        pub(super) fn new() -> Self {
            reset_shared_state();
            let mut buf = Box::new(crate::buffer_defs::BufT::default());
            let mut win = Box::new(crate::buffer_defs::WinT::default());
            let mut tab = Box::new(crate::buffer_defs::TabpageT::default());
            let buf_ptr = buf.as_mut() as *mut crate::buffer_defs::BufT;
            let win_ptr = win.as_mut() as *mut crate::buffer_defs::WinT;
            let tab_ptr = tab.as_mut() as *mut crate::buffer_defs::TabpageT;

            // Wire up b_vars/w_vars/tp_vars + b_bufvar/w_winvar/
            // tp_winvar exactly like the real buflist_new/win_alloc/
            // alloc_tabpage do: tv_dict_alloc() the scope dict, then
            // init_var_dict() to link the ScopeDictDictItem to it -
            // never a bare Default (which would leave di_tv at
            // Unknown, a state a real, fully-constructed buffer/
            // window/tabpage can never actually be in).
            unsafe {
                (*buf_ptr).b_vars = crate::eval::typval::tv_dict_alloc();
                init_var_dict(&mut *(*buf_ptr).b_vars, &mut (*buf_ptr).b_bufvar, ScopeType::Scope);
                (*win_ptr).w_vars = crate::eval::typval::tv_dict_alloc();
                init_var_dict(&mut *(*win_ptr).w_vars, &mut (*win_ptr).w_winvar, ScopeType::Scope);
                (*tab_ptr).tp_vars = crate::eval::typval::tv_dict_alloc();
                init_var_dict(&mut *(*tab_ptr).tp_vars, &mut (*tab_ptr).tp_winvar, ScopeType::Scope);
                (*win_ptr).w_buffer = buf_ptr;
                (*win_ptr).handle = 1;
                (*buf_ptr).handle = 1;
            }

            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let prev_curbuf = g.curbuf;
            let prev_curwin = g.curwin;
            let prev_curtab = g.curtab;
            let prev_firstwin = g.firstwin;
            let prev_first_tabpage = g.first_tabpage;
            let prev_lastbuf = g.lastbuf;
            g.curbuf = buf_ptr;
            g.curwin = win_ptr;
            g.curtab = tab_ptr;
            g.firstwin = win_ptr;
            g.first_tabpage = tab_ptr;
            g.lastbuf = buf_ptr;
            Self {
                buf,
                win,
                tab,
                prev_curbuf,
                prev_curwin,
                prev_curtab,
                prev_firstwin,
                prev_first_tabpage,
                prev_lastbuf,
            }
        }

        pub(super) fn buf_ptr(&mut self) -> *mut crate::buffer_defs::BufT {
            self.buf.as_mut() as *mut crate::buffer_defs::BufT
        }

        pub(super) fn win_ptr(&mut self) -> *mut crate::buffer_defs::WinT {
            self.win.as_mut() as *mut crate::buffer_defs::WinT
        }

        pub(super) fn tab_ptr(&mut self) -> *mut crate::buffer_defs::TabpageT {
            self.tab.as_mut() as *mut crate::buffer_defs::TabpageT
        }
    }

    impl Drop for TestFixture {
        fn drop(&mut self) {
            // Restore GLOBALS.curbuf/curwin/curtab/firstwin/
            // first_tabpage/lastbuf to their PRE-fixture values before
            // this fixture's own buf/win/tab Boxes are freed below -
            // otherwise these shared globals are left dangling,
            // pointing at freed memory, for whichever unrelated test
            // runs next (matching TestCurBufWinTab's own established
            // save/restore pattern in this same file).
            unsafe {
                let g = crate::globals::GLOBALS.get_mut();
                g.curbuf = self.prev_curbuf;
                g.curwin = self.prev_curwin;
                g.curtab = self.prev_curtab;
                g.firstwin = self.prev_firstwin;
                g.first_tabpage = self.prev_first_tabpage;
                g.lastbuf = self.prev_lastbuf;
            }
            // Free the real tv_dict_alloc()-backed b_vars/w_vars/
            // tp_vars dicts (unlinking them from GC_FIRST_DICT) before
            // the Boxes themselves drop - otherwise these leak into
            // the shared GC list forever, corrupting other tests that
            // assert on its contents (the exact leak class this
            // session has repeatedly caught and fixed elsewhere).
            unsafe {
                if !self.buf.b_vars.is_null() {
                    crate::eval::typval::tv_dict_free(self.buf.b_vars);
                }
                if !self.win.w_vars.is_null() {
                    crate::eval::typval::tv_dict_free(self.win.w_vars);
                }
                if !self.tab.tp_vars.is_null() {
                    crate::eval::typval::tv_dict_free(self.tab.tp_vars);
                }
            }
            reset_shared_state();
        }
    }

    #[test]
    fn get_var_from_finds_a_buffer_local_variable() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = TestFixture::new();
        let bp = fx.buf_ptr();
        let wp = fx.win_ptr();
        let tp = fx.tab_ptr();
        let item = crate::eval::typval::tv_dict_item_alloc(b"myvar");
        unsafe { (*item).di_tv.value = TypvalValue::Number(42) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *(*bp).b_vars, item) };

        let mut rettv = TypvalT::default();
        let deftv = TypvalT::default();
        unsafe { get_var_from(Some(b"myvar"), &mut rettv, &deftv, b'b', tp, wp, bp) };
        assert_eq!(rettv.value, TypvalValue::Number(42));
    }

    #[test]
    fn get_var_from_uses_default_when_variable_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = TestFixture::new();
        let bp = fx.buf_ptr();
        let wp = fx.win_ptr();
        let tp = fx.tab_ptr();

        let mut rettv = TypvalT::default();
        let deftv = TypvalT { value: TypvalValue::Number(99), ..TypvalT::default() };
        unsafe { get_var_from(Some(b"nope"), &mut rettv, &deftv, b'b', tp, wp, bp) };
        assert_eq!(rettv.value, TypvalValue::Number(99));
    }

    #[test]
    fn get_var_from_null_buf_with_htname_b_uses_default() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = TestFixture::new();
        let wp = fx.win_ptr();
        let tp = fx.tab_ptr();

        let mut rettv = TypvalT::default();
        let deftv = TypvalT { value: TypvalValue::Number(7), ..TypvalT::default() };
        unsafe {
            get_var_from(Some(b"myvar"), &mut rettv, &deftv, b'b', tp, wp, std::ptr::null_mut());
        }
        assert_eq!(rettv.value, TypvalValue::Number(7));
    }

    #[test]
    fn get_var_from_empty_varname_returns_the_whole_scope_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = TestFixture::new();
        let bp = fx.buf_ptr();
        let wp = fx.win_ptr();
        let tp = fx.tab_ptr();

        let mut rettv = TypvalT::default();
        let deftv = TypvalT::default();
        unsafe { get_var_from(Some(b""), &mut rettv, &deftv, b'b', tp, wp, bp) };
        assert_eq!(rettv.value, unsafe { (*bp).b_bufvar.di_tv.value.clone() });
    }

    #[test]
    fn get_var_from_option_name_reads_a_real_option() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = TestFixture::new();
        let bp = fx.buf_ptr();
        let wp = fx.win_ptr();
        let tp = fx.tab_ptr();

        let mut rettv = TypvalT::default();
        let deftv = TypvalT::default();
        // "&ignorecase" is global-only (option::get_varp_from resolves
        // it independent of curbuf/curwin), so this exercises the real
        // eval_option call without needing any option table wiring on
        // the test fixture's own buf/win.
        unsafe { get_var_from(Some(b"&ignorecase"), &mut rettv, &deftv, b'b', tp, wp, bp) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn get_var_from_tabname_option_prefix_is_not_treated_as_an_option() {
        // htname == 't' skips the "&option" special case entirely,
        // per get_var_from's own real control flow - "&foo" is looked
        // up as a literal (never-found) variable name instead.
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = TestFixture::new();
        let wp = fx.win_ptr();
        let tp = fx.tab_ptr();

        let mut rettv = TypvalT::default();
        let deftv = TypvalT { value: TypvalValue::Number(5), ..TypvalT::default() };
        unsafe {
            get_var_from(Some(b"&ignorecase"), &mut rettv, &deftv, b't', tp, wp, std::ptr::null_mut());
        }
        assert_eq!(rettv.value, TypvalValue::Number(5));
    }

    /// Sets `GLOBALS.curbuf`/`curwin`/`curtab` to freshly-allocated,
    /// linked structs (`win.w_buffer`/`w_s` point at `buf`/a real
    /// `SynblockT`) for the bare `"&"` whole-options-dict tests below,
    /// restoring the previous values and freeing everything on drop.
    ///
    /// Deliberately built via `Box::into_raw` directly (never
    /// `Box::new` + `.as_mut()` + separately storing the `Box`, as
    /// `TestFixture` above does) - the latter creates a SEPARATE
    /// reborrow tag from whatever gets stored into `GLOBALS`, which
    /// trips a real Tree Borrows "double reborrow" violation the
    /// moment a later write goes through a DIFFERENT reborrow of the
    /// same allocation than the one `GLOBALS` holds. This matters
    /// specifically here because `get_winbuf_options` (invoked
    /// internally by `get_var_from`'s bare `"&"` branch) reads through
    /// `GLOBALS.curbuf`/`curwin` directly, not through any explicit
    /// parameter - unlike every other branch this file's `TestFixture`
    /// was designed for, which only ever reads through the parameters
    /// `get_var_from` is explicitly called with. Matches
    /// `find_var_ht_dict`'s own `TestCurBufWinTab` established
    /// precedent (same file, different `mod tests` block) for this
    /// exact `Box::into_raw`-based pattern. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime.
    struct OptDictTestFixture {
        buf: *mut crate::buffer_defs::BufT,
        win: *mut crate::buffer_defs::WinT,
        tab: *mut crate::buffer_defs::TabpageT,
        syn: *mut crate::buffer_defs::SynblockT,
        prev_curbuf: *mut crate::buffer_defs::BufT,
        prev_curwin: *mut crate::buffer_defs::WinT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
    }

    impl OptDictTestFixture {
        fn new() -> Self {
            let buf = Box::into_raw(Box::new(crate::buffer_defs::BufT::default()));
            let syn = Box::into_raw(Box::new(crate::buffer_defs::SynblockT::default()));
            let win_val = crate::buffer_defs::WinT {
                w_buffer: buf,
                w_s: syn,
                ..Default::default()
            };
            let win = Box::into_raw(Box::new(win_val));
            let tab = Box::into_raw(Box::new(crate::buffer_defs::TabpageT::default()));

            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let prev_curbuf = g.curbuf;
            let prev_curwin = g.curwin;
            let prev_curtab = g.curtab;
            g.curbuf = buf;
            g.curwin = win;
            g.curtab = tab;
            OptDictTestFixture { buf, win, tab, syn, prev_curbuf, prev_curwin, prev_curtab }
        }
    }

    impl Drop for OptDictTestFixture {
        fn drop(&mut self) {
            unsafe {
                let g = crate::globals::GLOBALS.get_mut();
                g.curbuf = self.prev_curbuf;
                g.curwin = self.prev_curwin;
                g.curtab = self.prev_curtab;
                drop(Box::from_raw(self.buf));
                drop(Box::from_raw(self.win));
                drop(Box::from_raw(self.tab));
                drop(Box::from_raw(self.syn));
            }
        }
    }

    #[test]
    fn get_var_from_bare_ampersand_returns_a_real_buffer_local_options_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = OptDictTestFixture::new();
        unsafe { (*fx.buf).b_p_ts = 12 };

        let mut rettv = TypvalT::default();
        let deftv = TypvalT::default();
        unsafe { get_var_from(Some(b"&"), &mut rettv, &deftv, b'b', fx.tab, fx.win, fx.buf) };

        let TypvalValue::Dict(d) = rettv.value else {
            panic!("expected a Dict value, got {:?}", rettv.value);
        };
        assert!(!d.is_null());
        assert_eq!(unsafe { (*d).dv_refcount }, 1);
        let item = crate::eval::typval::tv_dict_find(Some(unsafe { &mut *d }), b"tabstop")
            .expect("'tabstop' should be present in a buffer-local options dict");
        assert_eq!(unsafe { &(*item).di_tv }.value, TypvalValue::Number(12));
        // A window-local-only option must NOT appear in the
        // buffer-local dict.
        assert!(crate::eval::typval::tv_dict_find(Some(unsafe { &mut *d }), b"wrap").is_none());

        unsafe { crate::eval::typval::tv_dict_unref(d) };
    }

    #[test]
    fn get_var_from_bare_ampersand_returns_a_real_window_local_options_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = OptDictTestFixture::new();
        unsafe { (*fx.win).w_onebuf_opt.wo_wrap = 1 };

        let mut rettv = TypvalT::default();
        let deftv = TypvalT::default();
        unsafe {
            get_var_from(Some(b"&"), &mut rettv, &deftv, b'w', fx.tab, fx.win, std::ptr::null_mut());
        }

        let TypvalValue::Dict(d) = rettv.value else {
            panic!("expected a Dict value, got {:?}", rettv.value);
        };
        assert!(!d.is_null());
        let item = crate::eval::typval::tv_dict_find(Some(unsafe { &mut *d }), b"wrap")
            .expect("'wrap' should be present in a window-local options dict");
        assert_eq!(unsafe { &(*item).di_tv }.value, TypvalValue::Number(1));
        // A buffer-local-only option must NOT appear in the
        // window-local dict.
        assert!(crate::eval::typval::tv_dict_find(Some(unsafe { &mut *d }), b"tabstop").is_none());

        unsafe { crate::eval::typval::tv_dict_unref(d) };
    }

    #[test]
    #[should_panic(expected = "ctx_switch")]
    fn get_var_from_a_different_window_panics() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = TestFixture::new();
        let mut other_win = crate::buffer_defs::WinT::default();
        let tp = fx.tab_ptr();
        let other_ptr = &mut other_win as *mut crate::buffer_defs::WinT;

        let mut rettv = TypvalT::default();
        let deftv = TypvalT::default();
        unsafe {
            get_var_from(Some(b"myvar"), &mut rettv, &deftv, b'w', tp, other_ptr, std::ptr::null_mut());
        }
    }

    // ---- getwinvar/gettabvar/gettabwinvar/getbufvar (the real
    // "f_*" entry points) ----

    #[test]
    fn f_getwinvar_reads_the_current_windows_own_variable() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = TestFixture::new();
        let wp = fx.win_ptr();
        let item = crate::eval::typval::tv_dict_item_alloc(b"myvar");
        unsafe { (*item).di_tv.value = TypvalValue::Number(3) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *(*wp).w_vars, item) };

        let argvars = [
            TypvalT { value: TypvalValue::Number(0), ..TypvalT::default() },
            TypvalT { value: TypvalValue::String(Some(b"myvar".to_vec())), ..TypvalT::default() },
        ];
        let mut rettv = TypvalT::default();
        unsafe { f_getwinvar(&argvars, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(3));
    }

    #[test]
    fn f_gettabvar_reads_the_current_tabpages_own_variable() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = TestFixture::new();
        let tp = fx.tab_ptr();
        let item = crate::eval::typval::tv_dict_item_alloc(b"myvar");
        unsafe { (*item).di_tv.value = TypvalValue::Number(9) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *(*tp).tp_vars, item) };

        let argvars = [
            TypvalT { value: TypvalValue::Number(1), ..TypvalT::default() },
            TypvalT { value: TypvalValue::String(Some(b"myvar".to_vec())), ..TypvalT::default() },
        ];
        let mut rettv = TypvalT::default();
        unsafe { f_gettabvar(&argvars, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(9));
    }

    #[test]
    fn f_gettabwinvar_reads_the_current_tab_and_windows_own_variable() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = TestFixture::new();
        let wp = fx.win_ptr();
        let item = crate::eval::typval::tv_dict_item_alloc(b"myvar");
        unsafe { (*item).di_tv.value = TypvalValue::Number(11) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *(*wp).w_vars, item) };

        let argvars = [
            TypvalT { value: TypvalValue::Number(1), ..TypvalT::default() },
            TypvalT { value: TypvalValue::Number(1), ..TypvalT::default() },
            TypvalT { value: TypvalValue::String(Some(b"myvar".to_vec())), ..TypvalT::default() },
        ];
        let mut rettv = TypvalT::default();
        unsafe { f_gettabwinvar(&argvars, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(11));
    }

    #[test]
    fn f_getbufvar_reads_a_buffer_local_variable_by_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = TestFixture::new();
        let bp = fx.buf_ptr();
        let item = crate::eval::typval::tv_dict_item_alloc(b"myvar");
        unsafe { (*item).di_tv.value = TypvalValue::Number(13) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *(*bp).b_vars, item) };

        let argvars = [
            TypvalT { value: TypvalValue::Number(1), ..TypvalT::default() },
            TypvalT { value: TypvalValue::String(Some(b"myvar".to_vec())), ..TypvalT::default() },
        ];
        let mut rettv = TypvalT::default();
        unsafe { f_getbufvar(&argvars, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(13));
    }

    #[test]
    fn f_getbufvar_falls_back_to_the_default_for_an_unknown_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = TestFixture::new();

        let argvars = [
            TypvalT { value: TypvalValue::Number(999), ..TypvalT::default() },
            TypvalT { value: TypvalValue::String(Some(b"myvar".to_vec())), ..TypvalT::default() },
            TypvalT { value: TypvalValue::Number(-1), ..TypvalT::default() },
        ];
        let mut rettv = TypvalT::default();
        unsafe { f_getbufvar(&argvars, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }
}

#[cfg(test)]
mod set_var_tests {
    use super::get_var_from_tests::TestFixture;
    use super::*;

    fn reset_shared_state() {
        crate::eval::userfunc::set_current_funccal(std::ptr::null_mut());
        unsafe { vars_clear(GLOBVARDICT.get_mut()) };
        crate::runtime::tests_reset_for_test();
        unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx = Default::default();
    }

    #[test]
    fn set_var_adds_a_new_global_variable() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        let mut tv = TypvalT { value: TypvalValue::Number(42), ..TypvalT::default() };
        unsafe { set_var(b"g:myvar", &mut tv, true) };

        let mut rettv = TypvalT::default();
        assert_eq!(unsafe { eval_variable(b"g:myvar", Some(&mut rettv), true, false) }, crate::vim_defs::OK);
        assert_eq!(rettv.value, TypvalValue::Number(42));

        reset_shared_state();
    }

    #[test]
    fn set_var_copy_true_leaves_tv_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        let mut tv = TypvalT { value: TypvalValue::Number(7), ..TypvalT::default() };
        unsafe { set_var(b"g:myvar", &mut tv, true) };
        // copy=true: the caller's own tv is untouched (still Number(7)).
        assert_eq!(tv.value, TypvalValue::Number(7));

        reset_shared_state();
    }

    #[test]
    fn set_var_copy_false_moves_and_resets_tv_to_unknown() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        let mut tv = TypvalT { value: TypvalValue::String(Some(b"hi".to_vec())), ..TypvalT::default() };
        unsafe { set_var(b"g:myvar", &mut tv, false) };
        // copy=false: the caller's own tv is reset to Unknown (moved out).
        assert_eq!(tv.value, TypvalValue::Unknown);

        let mut rettv = TypvalT::default();
        assert_eq!(unsafe { eval_variable(b"g:myvar", Some(&mut rettv), true, false) }, crate::vim_defs::OK);
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hi".to_vec())));

        reset_shared_state();
    }

    #[test]
    fn set_var_updates_an_existing_variable_releasing_the_old_value() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        let l = crate::eval::typval::tv_list_alloc(0);
        unsafe { crate::eval::typval::tv_list_ref(l) };
        let mut old_tv = TypvalT { value: TypvalValue::List(l), ..TypvalT::default() };
        unsafe { set_var(b"g:myvar", &mut old_tv, true) };
        assert_eq!(unsafe { (*l).lv_refcount }, 2); // 1 (ref'd above) + 1 (set_var's own copy)

        let mut new_tv = TypvalT { value: TypvalValue::Number(1), ..TypvalT::default() };
        unsafe { set_var(b"g:myvar", &mut new_tv, true) };
        // The list's refcount drops back to 1 (set_var's own copy released),
        // since g:myvar no longer references it.
        assert_eq!(unsafe { (*l).lv_refcount }, 1);

        unsafe { crate::eval::typval::tv_list_unref(l) };
        reset_shared_state();
    }

    #[test]
    fn set_var_readonly_variable_is_not_overwritten() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        let mut tv = TypvalT { value: TypvalValue::Number(1), ..TypvalT::default() };
        unsafe { set_var(b"g:myvar", &mut tv, true) };
        // Directly flag the item read-only, matching a real :lockvar's
        // own effect, without needing that command translated.
        let found = unsafe { find_var_in_ht(get_globvar_dict(), b'g', b"myvar", false) };
        let Some(DictitemVariant::Dict(item)) = found else { panic!("expected a Dict item") };
        unsafe { (*item).di_flags |= dict_item_flags::RO };

        let mut new_tv = TypvalT { value: TypvalValue::Number(99), ..TypvalT::default() };
        unsafe { set_var(b"g:myvar", &mut new_tv, true) };

        let mut rettv = TypvalT::default();
        assert_eq!(unsafe { eval_variable(b"g:myvar", Some(&mut rettv), true, false) }, crate::vim_defs::OK);
        assert_eq!(rettv.value, TypvalValue::Number(1), "read-only variable must not be overwritten");

        reset_shared_state();
    }

    #[test]
    fn set_var_const_locks_the_new_variable() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        let mut tv = TypvalT { value: TypvalValue::Number(1), ..TypvalT::default() };
        unsafe { set_var_const(b"g:myconst", &mut tv, true, true) };

        let found = unsafe { find_var_in_ht(get_globvar_dict(), b'g', b"myconst", false) };
        let Some(DictitemVariant::Dict(item)) = found else { panic!("expected a Dict item") };
        assert_ne!(unsafe { (*item).di_flags } & dict_item_flags::LOCK, 0);

        // A second set_var_const attempt on the now-const variable is
        // silently ignored (matching the original's own "emsg,
        // return" - message display omitted, the identical no-op
        // behavior is kept).
        let mut tv2 = TypvalT { value: TypvalValue::Number(2), ..TypvalT::default() };
        unsafe { set_var_const(b"g:myconst", &mut tv2, true, true) };
        let mut rettv = TypvalT::default();
        assert_eq!(unsafe { eval_variable(b"g:myconst", Some(&mut rettv), true, false) }, crate::vim_defs::OK);
        assert_eq!(rettv.value, TypvalValue::Number(1));

        reset_shared_state();
    }

    #[test]
    fn set_var_invalid_name_is_a_silent_no_op() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        // "1abc" has a leading digit - not a valid Vimscript identifier.
        let mut tv = TypvalT { value: TypvalValue::Number(1), ..TypvalT::default() };
        unsafe { set_var(b"g:1abc", &mut tv, true) };

        assert_eq!(unsafe { eval_variable(b"g:1abc", None, false, false) }, crate::vim_defs::FAIL);

        reset_shared_state();
    }

    #[test]
    #[should_panic(expected = "var_wrong_func_name")]
    fn set_var_with_a_funcref_value_panics() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        let mut tv = TypvalT {
            value: TypvalValue::Func(Some(b"SomeFunc".to_vec())),
            ..TypvalT::default()
        };
        unsafe { set_var(b"g:myvar", &mut tv, true) };
    }

    // ---- set_internal_string_var ----

    #[test]
    fn set_internal_string_var_creates_a_new_string_variable() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        unsafe { set_internal_string_var(b"g:current_compiler", Some(b"rustc")) };

        let mut rettv = TypvalT::default();
        assert_eq!(unsafe { eval_variable(b"g:current_compiler", Some(&mut rettv), true, false) }, crate::vim_defs::OK);
        assert_eq!(rettv.value, TypvalValue::String(Some(b"rustc".to_vec())));

        reset_shared_state();
    }

    #[test]
    fn set_internal_string_var_none_value_stores_a_null_string() {
        let _lock = crate::globals::global_state_test_lock();
        // b: scope resolution dereferences GLOBALS.curbuf directly -
        // needs a real fixture, not just reset_shared_state().
        let _fx = TestFixture::new();

        // Models a real call shape (e.g. syntax.c's "no previous
        // 'syntax' value" case) - the original's own char *value can
        // genuinely be NULL here, not just for name.
        unsafe { set_internal_string_var(b"b:current_syntax", None) };

        let mut rettv = TypvalT::default();
        assert_eq!(unsafe { eval_variable(b"b:current_syntax", Some(&mut rettv), true, false) }, crate::vim_defs::OK);
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    #[test]
    fn set_internal_string_var_overwrites_an_existing_value() {
        let _lock = crate::globals::global_state_test_lock();
        // w: scope resolution dereferences GLOBALS.curwin directly -
        // needs a real fixture, not just reset_shared_state().
        let _fx = TestFixture::new();

        unsafe { set_internal_string_var(b"w:quickfix_title", Some(b"first")) };
        unsafe { set_internal_string_var(b"w:quickfix_title", Some(b"second")) };

        let mut rettv = TypvalT::default();
        assert_eq!(unsafe { eval_variable(b"w:quickfix_title", Some(&mut rettv), true, false) }, crate::vim_defs::OK);
        assert_eq!(rettv.value, TypvalValue::String(Some(b"second".to_vec())));
    }

    // ---- setwinvar/settabwinvar/setbufvar (the real "f_*" entry
    // points) ----

    #[test]
    fn f_setwinvar_sets_the_current_windows_own_variable() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = TestFixture::new();
        let wp = fx.win_ptr();

        let argvars = [
            TypvalT { value: TypvalValue::Number(0), ..TypvalT::default() },
            TypvalT { value: TypvalValue::String(Some(b"myvar".to_vec())), ..TypvalT::default() },
            TypvalT { value: TypvalValue::Number(5), ..TypvalT::default() },
        ];
        let mut rettv = TypvalT::default();
        unsafe { f_setwinvar(&argvars, &mut rettv) };

        let found = unsafe { find_var_in_ht((*wp).w_vars, b'w', b"myvar", false) };
        let Some(DictitemVariant::Dict(item)) = found else { panic!("expected a Dict item") };
        assert_eq!(unsafe { (*item).di_tv.value.clone() }, TypvalValue::Number(5));
    }

    #[test]
    fn f_settabwinvar_sets_the_current_tab_and_windows_own_variable() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = TestFixture::new();
        let wp = fx.win_ptr();

        let argvars = [
            TypvalT { value: TypvalValue::Number(1), ..TypvalT::default() },
            TypvalT { value: TypvalValue::Number(0), ..TypvalT::default() },
            TypvalT { value: TypvalValue::String(Some(b"myvar".to_vec())), ..TypvalT::default() },
            TypvalT { value: TypvalValue::Number(11), ..TypvalT::default() },
        ];
        let mut rettv = TypvalT::default();
        unsafe { f_settabwinvar(&argvars, &mut rettv) };

        let found = unsafe { find_var_in_ht((*wp).w_vars, b'w', b"myvar", false) };
        let Some(DictitemVariant::Dict(item)) = found else { panic!("expected a Dict item") };
        assert_eq!(unsafe { (*item).di_tv.value.clone() }, TypvalValue::Number(11));
    }

    #[test]
    #[should_panic(expected = "ctx_switch")]
    fn f_setwinvar_a_different_window_panics() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();

        // Built from scratch (not via TestFixture) to keep every raw
        // pointer's own Tree Borrows lineage simple: each Box's
        // pointer is taken EXACTLY ONCE, immediately after
        // construction, and every subsequent mutation goes exclusively
        // through that one already-taken pointer - never re-derived a
        // second time via a fresh `.as_mut()`/`GLOBALS.get_mut()`
        // read-then-reborrow, which caught a real Tree Borrows
        // violation via `cargo miri test` in an earlier draft of this
        // test.
        let mut buf = Box::new(crate::buffer_defs::BufT::default());
        let mut win1 = Box::new(crate::buffer_defs::WinT::default());
        let mut win2 = Box::new(crate::buffer_defs::WinT::default());
        let mut tab = Box::new(crate::buffer_defs::TabpageT::default());
        let buf_ptr = buf.as_mut() as *mut crate::buffer_defs::BufT;
        let win1_ptr = win1.as_mut() as *mut crate::buffer_defs::WinT;
        let win2_ptr = win2.as_mut() as *mut crate::buffer_defs::WinT;
        let tab_ptr = tab.as_mut() as *mut crate::buffer_defs::TabpageT;

        unsafe {
            (*buf_ptr).handle = 1;
            (*win1_ptr).w_buffer = buf_ptr;
            (*win1_ptr).handle = 1;
            (*win1_ptr).w_next = win2_ptr;
            (*win2_ptr).w_buffer = buf_ptr;
            (*win2_ptr).handle = 2;
        }

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curbuf = buf_ptr;
        g.curwin = win1_ptr;
        g.curtab = tab_ptr;
        g.firstwin = win1_ptr;
        g.first_tabpage = tab_ptr;

        let argvars = [
            TypvalT { value: TypvalValue::Number(2), ..TypvalT::default() },
            TypvalT { value: TypvalValue::String(Some(b"myvar".to_vec())), ..TypvalT::default() },
            TypvalT { value: TypvalValue::Number(1), ..TypvalT::default() },
        ];
        let mut rettv = TypvalT::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            f_setwinvar(&argvars, &mut rettv);
        }));

        // Restore GLOBALS before re-raising the panic, matching
        // TestFixture's own Drop discipline - important even in a
        // #[should_panic] test, since other tests share these globals.
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curbuf = std::ptr::null_mut();
        g.curwin = std::ptr::null_mut();
        g.curtab = std::ptr::null_mut();
        g.firstwin = std::ptr::null_mut();
        g.first_tabpage = std::ptr::null_mut();
        reset_shared_state();

        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    #[should_panic(expected = "set_option_from_tv")]
    fn f_setwinvar_option_name_panics() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = TestFixture::new();

        let argvars = [
            TypvalT { value: TypvalValue::Number(0), ..TypvalT::default() },
            TypvalT { value: TypvalValue::String(Some(b"&number".to_vec())), ..TypvalT::default() },
            TypvalT { value: TypvalValue::Number(1), ..TypvalT::default() },
        ];
        let mut rettv = TypvalT::default();
        unsafe { f_setwinvar(&argvars, &mut rettv) };
    }

    #[test]
    fn f_setbufvar_sets_a_buffer_local_variable() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = TestFixture::new();
        let bp = fx.buf_ptr();

        let argvars = [
            TypvalT { value: TypvalValue::Number(1), ..TypvalT::default() },
            TypvalT { value: TypvalValue::String(Some(b"myvar".to_vec())), ..TypvalT::default() },
            TypvalT { value: TypvalValue::Number(17), ..TypvalT::default() },
        ];
        let mut rettv = TypvalT::default();
        unsafe { f_setbufvar(&argvars, &mut rettv) };

        let found = unsafe { find_var_in_ht((*bp).b_vars, b'b', b"myvar", false) };
        let Some(DictitemVariant::Dict(item)) = found else { panic!("expected a Dict item") };
        assert_eq!(unsafe { (*item).di_tv.value.clone() }, TypvalValue::Number(17));

        // curbuf must be restored to the fixture's own buffer
        // afterward, not left pointing at whatever setbufvar's own
        // temporary curbuf swap last set it to.
        assert!(std::ptr::eq(unsafe { crate::globals::GLOBALS.get_mut() }.curbuf, bp));
    }

    #[test]
    fn f_setbufvar_falls_back_silently_for_an_unknown_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = TestFixture::new();

        let argvars = [
            TypvalT { value: TypvalValue::Number(999), ..TypvalT::default() },
            TypvalT { value: TypvalValue::String(Some(b"myvar".to_vec())), ..TypvalT::default() },
            TypvalT { value: TypvalValue::Number(1), ..TypvalT::default() },
        ];
        let mut rettv = TypvalT::default();
        // Must not panic - buf resolves to null, early return.
        unsafe { f_setbufvar(&argvars, &mut rettv) };
    }

    #[test]
    #[should_panic(expected = "set_option_from_tv")]
    fn f_setbufvar_option_name_panics() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = TestFixture::new();

        let argvars = [
            // tv_get_buf's Number branch always resolves via
            // buflist_findnr(n) - it has no "0 means current buffer"
            // special case (unlike find_win_by_nr/find_tabpage) - so
            // this must use the fixture's own real buffer handle (1),
            // not a bare 0.
            TypvalT { value: TypvalValue::Number(1), ..TypvalT::default() },
            TypvalT { value: TypvalValue::String(Some(b"&number".to_vec())), ..TypvalT::default() },
            TypvalT { value: TypvalValue::Number(1), ..TypvalT::default() },
        ];
        let mut rettv = TypvalT::default();
        unsafe {
            f_setbufvar(&argvars, &mut rettv);
        }
    }
}

#[cfg(test)]
mod prepare_restore_vimvar_tests {
    use super::*;

    #[test]
    fn prepare_then_restore_a_normally_registered_variable_roundtrips() {
        let _lock = crate::globals::global_state_test_lock();
        // v:count is normally registered (VarType != Unknown) - its
        // hashtab/dv_index membership must be unaffected by
        // prepare/restore.
        let before_hash_used = unsafe { (*get_vimvar_dict()).dv_hashtab.ht_used };

        let mut save = TypvalT::default();
        unsafe { set_vim_var_nr(VimVarIndex::Count, 42) };
        unsafe { prepare_vimvar(VimVarIndex::Count, &mut save) };
        assert_eq!(save.value, TypvalValue::Number(42));

        unsafe { set_vim_var_nr(VimVarIndex::Count, 7) };
        assert_eq!(unsafe { get_vim_var_nr(VimVarIndex::Count) }, 7);

        unsafe { restore_vimvar(VimVarIndex::Count, save) };
        assert_eq!(unsafe { get_vim_var_nr(VimVarIndex::Count) }, 42);
        assert_eq!(unsafe { (*get_vimvar_dict()).dv_hashtab.ht_used }, before_hash_used);
    }

    #[test]
    fn prepare_vimvar_registers_val_in_the_hashtable_while_active() {
        let _lock = crate::globals::global_state_test_lock();
        // v:val is normally VAR_UNKNOWN and NOT in the hashtable.
        assert!(unsafe { hashitem_empty((*get_vimvar_dict()).dv_hashtab.hash_find(b"val")) });

        let mut save = TypvalT::default();
        unsafe { prepare_vimvar(VimVarIndex::Val, &mut save) };

        // Now genuinely findable via a real hash lookup, matching a
        // real Vimscript `v:val` reference during filter()/map().
        assert!(!unsafe { hashitem_empty((*get_vimvar_dict()).dv_hashtab.hash_find(b"val")) });

        unsafe { restore_vimvar(VimVarIndex::Val, save) };

        // Removed again afterward.
        assert!(unsafe { hashitem_empty((*get_vimvar_dict()).dv_hashtab.hash_find(b"val")) });
    }

    #[test]
    fn prepare_vimvar_registers_key_in_the_hashtable_while_active() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(unsafe { hashitem_empty((*get_vimvar_dict()).dv_hashtab.hash_find(b"key")) });

        let mut save = TypvalT::default();
        unsafe { prepare_vimvar(VimVarIndex::Key, &mut save) };
        assert!(!unsafe { hashitem_empty((*get_vimvar_dict()).dv_hashtab.hash_find(b"key")) });

        unsafe { restore_vimvar(VimVarIndex::Key, save) };
        assert!(unsafe { hashitem_empty((*get_vimvar_dict()).dv_hashtab.hash_find(b"key")) });
    }

    #[test]
    fn prepare_vimvar_is_reentrant_safe_for_nested_val_overrides() {
        let _lock = crate::globals::global_state_test_lock();
        // Simulates a nested filter()/map() call (an expression whose
        // own v:val use triggers ANOTHER filter() internally) - the
        // outer save/restore pair must not corrupt the inner one's own
        // hashtab registration.
        let mut save_outer = TypvalT::default();
        unsafe { prepare_vimvar(VimVarIndex::Val, &mut save_outer) };
        unsafe { set_vim_var_nr(VimVarIndex::Val, 1) };

        let mut save_inner = TypvalT::default();
        unsafe { prepare_vimvar(VimVarIndex::Val, &mut save_inner) };
        unsafe { set_vim_var_nr(VimVarIndex::Val, 2) };
        assert_eq!(unsafe { get_vim_var_nr(VimVarIndex::Val) }, 2);

        unsafe { restore_vimvar(VimVarIndex::Val, save_inner) };
        assert_eq!(unsafe { get_vim_var_nr(VimVarIndex::Val) }, 1);
        // Still registered: save_inner's own value (captured by the
        // inner prepare_vimvar, when v:val already held Number(1)) is
        // Number, not Unknown, so this restore's own
        // "remove-if-Unknown" gate does not fire - matching the
        // original's exact per-restore-call decision, made purely from
        // the value being restored, not any notion of "nesting depth".
        assert!(!unsafe { hashitem_empty((*get_vimvar_dict()).dv_hashtab.hash_find(b"val")) });

        unsafe { restore_vimvar(VimVarIndex::Val, save_outer) };
        assert!(unsafe { hashitem_empty((*get_vimvar_dict()).dv_hashtab.hash_find(b"val")) });
    }

    #[test]
    fn prepare_vimvar_survives_many_repeated_cycles() {
        let _lock = crate::globals::global_state_test_lock();
        for i in 0..200 {
            let mut save = TypvalT::default();
            unsafe { prepare_vimvar(VimVarIndex::Val, &mut save) };
            assert!(
                !unsafe { hashitem_empty((*get_vimvar_dict()).dv_hashtab.hash_find(b"val")) },
                "iteration {i}: val should be registered while active"
            );
            unsafe { set_vim_var_nr(VimVarIndex::Val, i) };
            assert_eq!(unsafe { get_vim_var_nr(VimVarIndex::Val) }, i, "iteration {i}: value mismatch");
            unsafe { restore_vimvar(VimVarIndex::Val, save) };
            assert!(
                unsafe { hashitem_empty((*get_vimvar_dict()).dv_hashtab.hash_find(b"val")) },
                "iteration {i}: val should be unregistered after restore"
            );
            assert_eq!(
                unsafe { (*get_vim_var_tv(VimVarIndex::Val)).value.var_type() },
                VarType::Unknown,
                "iteration {i}: value should be back to Unknown"
            );
        }
    }
}
