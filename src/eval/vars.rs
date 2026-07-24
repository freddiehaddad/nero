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
//! inspection. Each entry's own `tv` matches EXACTLY what the
//! original's macro produces BEFORE `evalvars_init()` ever runs (a
//! zero-valued `vval` of the entry's declared type - `Number(0)`,
//! `String(None)`, a null `List`/`Dict`/`Blob`/`Partial` pointer,
//! `Bool(BoolVarValue::False)`, or `Special(SpecialVarValue::Null)`).
//!
//! `evalvars_init` itself (which overrides several entries with real
//! startup values - `v:count1`/`v:hlsearch`/`v:searchforward` all
//! become `1`, `v:true` becomes `Bool(True)`, `v:errors` gets a real
//! empty list, etc. - and wires every entry into the real `v:` scope
//! dict via `hash_add`) is NOT yet translated: it needs `hash_add`
//! wired to a live `v:` scope `DictT` (not just the bare array here),
//! `tv_dict_alloc_lock`/`tv_list_set_lock` (not yet translated),
//! msgpack-type introspection (`msgpack_type_names`/
//! `eval_msgpack_type_lists`), and a Lua partial callback for `v:lua` -
//! each a genuinely separate undertaking from this table + its
//! accessors, left for a dedicated future pass. Until it lands, every
//! `VIMVARS` entry reads as its bare static-initializer default, NOT
//! its real runtime value - documented per-function below where this
//! matters.
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
//! Also translated: a new `GLOBVARDICT` file-static (the `g:` scope
//! dict, `globvardict` - never heap-allocated/GC-list-linked, matching
//! `new_script_vars`'s own precedent for the analogous script-scope
//! dict; currently reads as its bare, zero-valued PRE-`evalvars_init`
//! state, matching `VIMVARS`'s own already-documented limitation) plus
//! `get_globvar_dict` and `del_menutrans_vars` (filters `GLOBVARDICT`'s
//! own `dv_index` directly rather than the original's `HASHTAB_ITER`
//! walk, reusing the already-real `tv_dict_item_remove` in place of a
//! separate `delete_var` binding - both do exactly the same thing).
//! Building `GLOBVARDICT` also newly unlocks `get_globvar_dict`'s
//! future role as one of `find_var_ht_dict`'s many branches, though
//! that function (and `find_var`/`check_vars`/most of `ex_let`, all
//! genuinely substantial, needing `compat_hashtab`/`get_vimvar_dict`/
//! `curbuf.b_vars`-style fields/`current_sctx` script tracking) remain
//! deliberately NOT started - confirmed via direct reading of their
//! real bodies, not assumed.
//!
//! Deferred: everything else in this file (variable get/set/unlet,
//! `:let` parsing, `evalvars_init`, etc.).

use crate::eval::typval_defs::{
    dict_item_flags, BoolVarValue, DictT, DictitemT, ScidT, ScopeDictDictItem, ScopeType,
    SpecialVarValue, TypvalT, TypvalValue, VarLockStatus, VarType, VarnumberT, DO_NOT_FREE_CNT,
};
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
/// `vv_flags`/`vv_di.di_tv` only; `vv_di`'s own `di_flags`/`di_key`
/// fields are populated at runtime by `evalvars_init`, not yet
/// translated, so aren't modeled as separate fields here - see
/// [`get_vim_var_name`] for how `di_key`'s role, looking up a
/// variable's own name, is served directly by `name` instead).
struct Vimvar {
    /// Name of the variable, without `v:` (`vv_name`).
    name: &'static str,
    /// Flags: some combination of [`vv_flag::COMPAT`]/
    /// [`vv_flag::RO`]/[`vv_flag::RO_SBX`] (`vv_flags`).
    #[allow(dead_code)] // not yet read by any translated function - evalvars_init (not yet translated) is the original's only reader
    flags: u8,
    /// Value and lock status (`vv_di.di_tv`).
    tv: TypvalT,
}

/// The `v:` variable table (`vimvars[]`). See this module's own doc
/// comment for the full explanation of this table's construction,
/// indexing, and relationship to the NOT-yet-translated
/// `evalvars_init`.
static VIMVARS: std::sync::LazyLock<crate::globals::GlobalCell<Vec<Vimvar>>> =
    std::sync::LazyLock::new(|| {
        crate::globals::GlobalCell::new(vec![
    // VV_COUNT
    Vimvar { name: "count", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_COUNT1
    Vimvar { name: "count1", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_PREVCOUNT
    Vimvar { name: "prevcount", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_ERRMSG
    Vimvar { name: "errmsg", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_WARNINGMSG
    Vimvar { name: "warningmsg", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_STATUSMSG
    Vimvar { name: "statusmsg", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_SHELL_ERROR
    Vimvar { name: "shell_error", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_THIS_SESSION
    Vimvar { name: "this_session", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_VERSION
    Vimvar { name: "version", flags: vv_flag::COMPAT | vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_LNUM
    Vimvar { name: "lnum", flags: vv_flag::RO_SBX, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_TERMREQUEST
    Vimvar { name: "termrequest", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_TERMRESPONSE
    Vimvar { name: "termresponse", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_FNAME
    Vimvar { name: "fname", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_LANG
    Vimvar { name: "lang", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_LC_TIME
    Vimvar { name: "lc_time", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_CTYPE
    Vimvar { name: "ctype", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_CC_FROM
    Vimvar { name: "charconvert_from", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_CC_TO
    Vimvar { name: "charconvert_to", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_FNAME_IN
    Vimvar { name: "fname_in", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_FNAME_OUT
    Vimvar { name: "fname_out", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_FNAME_NEW
    Vimvar { name: "fname_new", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_FNAME_DIFF
    Vimvar { name: "fname_diff", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_CMDARG
    Vimvar { name: "cmdarg", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_FOLDSTART
    Vimvar { name: "foldstart", flags: vv_flag::RO_SBX, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_FOLDEND
    Vimvar { name: "foldend", flags: vv_flag::RO_SBX, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_FOLDDASHES
    Vimvar { name: "folddashes", flags: vv_flag::RO_SBX, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_FOLDLEVEL
    Vimvar { name: "foldlevel", flags: vv_flag::RO_SBX, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_PROGNAME
    Vimvar { name: "progname", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_SEND_SERVER
    Vimvar { name: "servername", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_DYING
    Vimvar { name: "dying", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_EXCEPTION
    Vimvar { name: "exception", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_THROWPOINT
    Vimvar { name: "throwpoint", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_REG
    Vimvar { name: "register", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_CMDBANG
    Vimvar { name: "cmdbang", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_INSERTMODE
    Vimvar { name: "insertmode", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_VAL
    Vimvar { name: "val", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Unknown } },
    // VV_KEY
    Vimvar { name: "key", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Unknown } },
    // VV_PROFILING
    Vimvar { name: "profiling", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_FCS_REASON
    Vimvar { name: "fcs_reason", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_FCS_CHOICE
    Vimvar { name: "fcs_choice", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_BEVAL_BUFNR
    Vimvar { name: "beval_bufnr", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_BEVAL_WINNR
    Vimvar { name: "beval_winnr", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_BEVAL_WINID
    Vimvar { name: "beval_winid", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_BEVAL_LNUM
    Vimvar { name: "beval_lnum", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_BEVAL_COL
    Vimvar { name: "beval_col", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_BEVAL_TEXT
    Vimvar { name: "beval_text", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_SCROLLSTART
    Vimvar { name: "scrollstart", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_SWAPNAME
    Vimvar { name: "swapname", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_SWAPCHOICE
    Vimvar { name: "swapchoice", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_SWAPCOMMAND
    Vimvar { name: "swapcommand", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_CHAR
    Vimvar { name: "char", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_MOUSE_WIN
    Vimvar { name: "mouse_win", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_MOUSE_WINID
    Vimvar { name: "mouse_winid", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_MOUSE_LNUM
    Vimvar { name: "mouse_lnum", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_MOUSE_COL
    Vimvar { name: "mouse_col", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_OP
    Vimvar { name: "operator", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_SEARCHFORWARD
    Vimvar { name: "searchforward", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_HLSEARCH
    Vimvar { name: "hlsearch", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_OLDFILES
    Vimvar { name: "oldfiles", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) } },
    // VV_WINDOWID
    Vimvar { name: "windowid", flags: vv_flag::RO_SBX, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_PROGPATH
    Vimvar { name: "progpath", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_COMPLETED_ITEM
    Vimvar { name: "completed_item", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(std::ptr::null_mut()) } },
    // VV_OPTION_NEW
    Vimvar { name: "option_new", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_OPTION_OLD
    Vimvar { name: "option_old", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_OPTION_OLDLOCAL
    Vimvar { name: "option_oldlocal", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_OPTION_OLDGLOBAL
    Vimvar { name: "option_oldglobal", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_OPTION_COMMAND
    Vimvar { name: "option_command", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_OPTION_TYPE
    Vimvar { name: "option_type", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_ERRORS
    Vimvar { name: "errors", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) } },
    // VV_FALSE
    Vimvar { name: "false", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Bool(BoolVarValue::False) } },
    // VV_TRUE
    Vimvar { name: "true", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Bool(BoolVarValue::False) } },
    // VV_NULL
    Vimvar { name: "null", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Special(SpecialVarValue::Null) } },
    // VV_NUMBERMAX
    Vimvar { name: "numbermax", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_NUMBERMIN
    Vimvar { name: "numbermin", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_NUMBERSIZE
    Vimvar { name: "numbersize", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_VIM_DID_ENTER
    Vimvar { name: "vim_did_enter", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_TESTING
    Vimvar { name: "testing", flags: 0, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_TYPE_NUMBER
    Vimvar { name: "t_number", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_TYPE_STRING
    Vimvar { name: "t_string", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_TYPE_FUNC
    Vimvar { name: "t_func", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_TYPE_LIST
    Vimvar { name: "t_list", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_TYPE_DICT
    Vimvar { name: "t_dict", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_TYPE_FLOAT
    Vimvar { name: "t_float", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_TYPE_BOOL
    Vimvar { name: "t_bool", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_TYPE_BLOB
    Vimvar { name: "t_blob", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_EVENT
    Vimvar { name: "event", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(std::ptr::null_mut()) } },
    // VV_VERSIONLONG
    Vimvar { name: "versionlong", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_ECHOSPACE
    Vimvar { name: "echospace", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_ARGF
    Vimvar { name: "argf", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) } },
    // VV_ARGV
    Vimvar { name: "argv", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) } },
    // VV_COLLATE
    Vimvar { name: "collate", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_EXITING
    Vimvar { name: "exiting", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_MAXCOL
    Vimvar { name: "maxcol", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_STACKTRACE
    Vimvar { name: "stacktrace", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) } },
    // VV_VIM_DID_INIT
    Vimvar { name: "vim_did_init", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_STDERR
    Vimvar { name: "stderr", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_MSGPACK_TYPES
    Vimvar { name: "msgpack_types", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(std::ptr::null_mut()) } },
    // VV__NULL_STRING
    Vimvar { name: "_null_string", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV__NULL_LIST
    Vimvar { name: "_null_list", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) } },
    // VV__NULL_DICT
    Vimvar { name: "_null_dict", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(std::ptr::null_mut()) } },
    // VV__NULL_BLOB
    Vimvar { name: "_null_blob", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Blob(std::ptr::null_mut()) } },
    // VV_LUA
    Vimvar { name: "lua", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Partial(std::ptr::null_mut()) } },
    // VV_RELNUM
    Vimvar { name: "relnum", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_VIRTNUM
    Vimvar { name: "virtnum", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_STARTTIME
    Vimvar { name: "starttime", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_EXITREASON
    Vimvar { name: "exitreason", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
    // VV_USERACTIVE
    Vimvar { name: "useractive", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(0) } },
    // VV_STARTREASON
    Vimvar { name: "startreason", flags: vv_flag::RO, tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) } },
        ])
    });

/// The global (`g:`) scope dict (`globvardict`; `globvarht` is just
/// `globvardict.dv_hashtab` in the original, via a `#define`).
///
/// A `dict_T` file-static in the original - never heap-allocated,
/// hence never linked into `GC_FIRST_DICT`'s used-dicts list, matching
/// `new_script_vars`'s own already-established precedent for the
/// analogous script-scope dict (see its doc comment in
/// `crate::runtime`). Currently reads as the bare, zero-valued
/// PRE-`evalvars_init` state (`dv_refcount: 0`, an empty
/// `dv_hashtab`) - `evalvars_init`'s own
/// `init_var_dict(get_globvar_dict(), &globvars_var, VAR_DEF_SCOPE)`
/// call (not yet translated) is what would normally set
/// `dv_refcount = DO_NOT_FREE_CNT` and wire it up as a real scope
/// dict, matching `VIMVARS`'s own already-documented "reads as its
/// bare static-initializer default, NOT its real runtime value"
/// limitation until that lands.
static GLOBVARDICT: std::sync::LazyLock<crate::globals::GlobalCell<DictT>> =
    std::sync::LazyLock::new(|| {
        crate::globals::GlobalCell::new(DictT {
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
        })
    });

/// @return the global (`g:`) variable dictionary (`get_globvar_dict`).
#[must_use]
pub fn get_globvar_dict() -> *mut DictT {
    // SAFETY: GLOBVARDICT is only ever read/written through this
    // module's own functions, matching VIMVARS's own established
    // convention.
    unsafe { GLOBVARDICT.get_mut() as *mut DictT }
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
    // SAFETY: only touches this module's own GLOBVARDICT cell.
    let d = unsafe { GLOBVARDICT.get_mut() };
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

/// Get the name of `v:` variable `idx`, without the `v:` prefix
/// (`get_vim_var_name`).
#[must_use]
pub fn get_vim_var_name(idx: VimVarIndex) -> &'static str {
    // SAFETY: VIMVARS is only ever read/written through this module's
    // own functions, none of which hold a live reference across
    // another call into this same cell.
    let vimvars = unsafe { VIMVARS.get_mut() };
    vimvars[idx as usize].name
}

/// Get a raw pointer to `v:` variable `idx`'s own `typval_T`
/// (`get_vim_var_tv`).
///
/// # Safety
/// The returned pointer stays valid as long as `VIMVARS` itself
/// (the whole program's lifetime, in practice): its backing `Vec` is
/// populated once, with a fixed 108 entries, and never resized
/// afterward by any function in this module, so indexing into it can
/// never be invalidated by reallocation. Callers must still not
/// retain the returned pointer across any call that could conflict
/// with this crate's usual `GlobalCell` aliasing rule (no two live
/// mutable accesses to the same cell at once).
#[must_use]
pub unsafe fn get_vim_var_tv(idx: VimVarIndex) -> *mut TypvalT {
    // SAFETY: forwarded from this function's own safety doc.
    std::ptr::addr_of_mut!(unsafe { &mut *VIMVARS.get_mut() }[idx as usize].tv)
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
    let vimvars = unsafe { VIMVARS.get_mut() };
    vimvars[idx as usize].tv = tv;
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
    let vimvars = unsafe { VIMVARS.get_mut() };
    vimvars[idx as usize].tv.value = TypvalValue::Number(0);
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
    let vimvars = unsafe { VIMVARS.get_mut() };
    vimvars[idx as usize].tv.value = TypvalValue::Number(val);
}

/// Set boolean `v:` variable `idx` to `val` (`set_vim_var_bool`).
///
/// # Safety
/// Same as [`get_vim_var_tv`].
pub unsafe fn set_vim_var_bool(idx: VimVarIndex, val: BoolVarValue) {
    // SAFETY: forwarded from this function's own safety doc.
    let vimvars = unsafe { VIMVARS.get_mut() };
    vimvars[idx as usize].tv.value = TypvalValue::Bool(val);
}

/// Set special `v:` variable `idx` to `val` (`set_vim_var_special`).
///
/// # Safety
/// Same as [`get_vim_var_tv`].
pub unsafe fn set_vim_var_special(idx: VimVarIndex, val: SpecialVarValue) {
    // SAFETY: forwarded from this function's own safety doc.
    let vimvars = unsafe { VIMVARS.get_mut() };
    vimvars[idx as usize].tv.value = TypvalValue::Special(val);
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
    let vimvars = unsafe { VIMVARS.get_mut() };
    vimvars[idx as usize].tv.value = TypvalValue::String(val.map(<[u8]>::to_vec));
}

/// Set list `v:` variable `idx` to `val`. Reference count will be
/// incremented (`set_vim_var_list`).
///
/// # Safety
/// Same as [`get_vim_var_tv`]. `val`, if non-null, must be a valid
/// pointer to a live [`crate::eval::typval_defs::ListT`].
pub unsafe fn set_vim_var_list(idx: VimVarIndex, val: *mut crate::eval::typval_defs::ListT) {
    // SAFETY: forwarded from this function's own safety doc.
    let vimvars = unsafe { VIMVARS.get_mut() };
    vimvars[idx as usize].tv.value = TypvalValue::List(val);
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
    let vimvars = unsafe { VIMVARS.get_mut() };
    vimvars[idx as usize].tv.value = TypvalValue::Dict(val);
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
    let vimvars = unsafe { VIMVARS.get_mut() };
    vimvars[idx as usize].tv.value = TypvalValue::Partial(val);
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
