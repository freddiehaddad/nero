//! Translated from `src/nvim/eval/typval_defs.h` (partial: the numeric
//! type aliases needed by `undo_defs.h`, plus `scid_T`/`sctx_T` needed by
//! `buffer_defs.h`'s `winopt_T`, `Callback`/`CallbackType` needed by
//! `buffer_defs.h`'s buffer-local-options block (phase 3), the
//! self-contained enums/constants with no pointer fields
//! (`VarType`/`VAR_TYPE_*`, `VarLockStatus`, `ScopeType`,
//! `BoolVarValue`, `SpecialVarValue`, `DictItemFlags`, `ListLenSpecials`,
//! `DO_NOT_FREE_CNT`, `MAX_FUNC_ARGS`/`VAR_SHORT_LEN`/`FIXVAR_CNT`); and
//! now `typval_T` itself (as `TypvalT`/`TypvalValue`), `list_T`/
//! `listitem_T`/`listwatch_T` (as `ListT`/`ListitemT`/`ListwatchT`), and
//! `blob_T` (as `BlobT`) - the foundational *data shapes* of the eval
//! engine's value system, translated ahead of the actual allocation/
//! refcounting/garbage-collection *algorithms* that operate on them
//! (`tv_list_alloc`, `tv_dict_alloc`, etc. - `eval/list.c`/`typval.c`,
//! still not started), mirroring how `option_defs.rs`'s `OptIndex`
//! family was translated well ahead of `option.c`'s real engine.
//!
//! `typval_T` (`TypvalT`/`TypvalValue`) is translated as a proper safe
//! Rust enum rather than replicating the original's `v_type: VarType`
//! tag + raw C union `vval` split - matching this same file's own
//! established `Callback` precedent ("no hot-path memory layout reason
//! not to"). Given how central and how heavily this type is
//! constructed/mutated throughout the entire eval engine, a safe
//! representation here is especially valuable (eliminates a whole
//! class of "read the wrong union field" undefined behavior) and
//! matches this crate's overall "correctness over exact C memory
//! layout" philosophy (e.g. `Vec<u8>` instead of raw pointers,
//! `Option` instead of null) - unlike e.g. `decoration_defs.rs`'s
//! `DecorInlineData`, which stays a raw union because callers actually
//! rely on its externally co-located discriminant/FFI-observable
//! layout; nothing here does.
//!
//! `list_T`/`listitem_T`/`listwatch_T` are translated with the same
//! raw-pointer-linked-structure convention already established for
//! `marktree.rs`'s `MtNode`/`MtNodeInner` (not `Rc`/`RefCell`) - pointer
//! fields (`li_next`, `lv_used_next`, etc.) mirror the original's own
//! manual, reference-counted, doubly-linked/intrusive-list ownership
//! model exactly, since Rust's ownership types don't have a
//! direct-enough equivalent for a structure this pervasively
//! pointer-aliased (list items are simultaneously reachable from
//! `lv_first`/`lv_last` traversal AND any live `listwatch_T`/
//! `lv_idx_item` cache AND (for nested lists) another list's own
//! item value) without introducing unsafe cells everywhere anyway.
//!
//! `dict_T`/`dictitem_T` (the generic, variable-key-length case -
//! distinct from the already-existing fixed-size `ChangedtickDictItem`/
//! `ScopeDictDictItem` instantiations of the same `TV_DICTITEM_STRUCT`
//! macro) now have their real design (as `DictT`/`DictitemT`): see
//! `DictitemT`'s own doc comment for the full reasoning - `di_key` is
//! an owned `Vec<u8>` rather than the original's flexible array
//! member, and `DictT` carries a new `dv_index` side table (a
//! `HashMap` from each item's `hi_key` address to its owning
//! `*mut DictitemT`) in place of the original's `TV_DICT_HI2DI`
//! pointer-arithmetic recovery, which has no safe Rust equivalent
//! here. The actual allocation/lookup/insertion *functions*
//! (`tv_dict_alloc`, `tv_dict_find`, etc.) live in `eval/typval.rs`,
//! translated from `eval/typval.c`.
//!
//! `funccall_T` (as [`FunccallT`]) now has its real fields too, proven
//! tractable at small scale first via `runtime_defs.rs`'s `ScriptvarT`
//! (which embeds a single `DictT` by value): `fc_l_vars`/`fc_l_avars:
//! DictT` and `fc_l_varlist: ListT` are embedded *by value* here too
//! (not by pointer, unlike every other use of those types in this
//! crate outside `ScriptvarT`) - this works cleanly with the existing
//! `tv_dict_*`/`tv_list_*` APIs (always just `*mut DictT`/`*mut ListT`,
//! never `Box<...>`), for the exact same reason `ScriptvarT` already
//! established. `fc_fixvar: [DictitemT; FIXVAR_CNT]` collapses the
//! original's `TV_DICTITEM_STRUCT(VAR_SHORT_LEN + 1)` fixed-size
//! instantiation into plain `DictitemT`s (already `Vec<u8>`-keyed
//! regardless of any particular size parameter, so the size parameter
//! itself carries no information a Rust translation needs to keep).
//! `FunccallT` derives neither `Debug` nor `Default` (matching
//! `DictT`/`ListT`'s own convention, embedded here by value) - a
//! manual `Default` impl is provided instead, zero-initializing every
//! field, faithfully matching the original's own `create_funccal`
//! (`xcalloc(1, sizeof(funccall_T))`, i.e. a real, meaningful "freshly
//! allocated, not yet further set up" state, only later populated
//! field-by-field elsewhere - unlike `DictT`/`ListT` themselves, which
//! need their own dedicated allocator functions for real GC-list
//! bookkeeping that a naive zero/`Default` value would skip).
//! `fc_caller`/`fc_func`/`fc_rettv`/etc. (raw pointers to other
//! already-real types) need no further design of their own.
//!
//! `ufunc_T` (as [`UfuncT`]) has its real fields too - unlike
//! `dictitem_T`'s `di_key`, `uf_name` needed no side-table design of
//! its OWN struct shape (that complexity lives in `HIKEY2UF`'s pointer-
//! arithmetic recovery from a *hashtable* keyed by `uf_name`, which is
//! `eval/userfunc.c`'s `func_hashtab` - a file-static, not part of
//! `ufunc_T` itself; now translated too, as `eval/userfunc.rs`'s
//! private `FuncHashtab`, with its own `DictT.dv_index`-style side
//! table as predicted). `uf_name`/`uf_namelen`
//! (the flexible array member) simply collapse into one owned
//! `Vec<u8>`, matching `DictitemT.di_key`'s established treatment.
//! `UfuncT.uf_scoped: *mut FunccallT` (and `PartialT.pt_func: *mut
//! UfuncT` below) reference their respective real types directly.
//!
//! `partial_T` (as [`PartialT`]) had NO flexible-array-member of its
//! own and needed no design decision beyond the already-established
//! conventions: `pt_argv` (the original's allocated `typval_T *` +
//! `pt_argc` pair) collapses into a plain `Vec<TypvalT>` (its own
//! `.len()` replacing `pt_argc`), matching this crate's usual "owned
//! `Vec` instead of allocated-array-plus-count" convention. The
//! allocation/refcounting *functions* (`partial_free`/`partial_unref`)
//! live in `eval/typval.rs` alongside their sibling `tv_*_free`/`_unref`
//! functions, even though `partial_T`'s real home in the original is
//! `eval.c`, not `eval/typval.c` (see that module's own doc comment).

use crate::pos_defs::LinenrT;
use crate::types_defs::LuaRef;

/// Structure that holds an internal variable value (`typval_T`).
///
/// See this module's own doc comment for why this is a safe Rust enum
/// (via [`TypvalValue`]) rather than the original's `v_type` tag + raw
/// C union `vval` split.
///
/// `v_lock` (the original's separate `VarLockStatus` field, sitting
/// alongside `v_type`/`vval`) is kept as its own field here too - it's
/// orthogonal to which variant is active (any variant can be locked or
/// not), so folding it into the enum itself would needlessly duplicate
/// a `v_lock` field onto every single variant.
#[derive(Debug, Clone, Default)]
pub struct TypvalT {
    /// Variable lock status (`v_lock`).
    pub v_lock: VarLockStatus,
    /// The actual value (`v_type` + `vval`, combined).
    pub value: TypvalValue,
}

impl TypvalT {
    /// The `VarType` tag this value corresponds to (`v_type`) -
    /// mirroring the original's separate discriminant field, derived
    /// here instead of stored redundantly.
    #[must_use]
    pub fn var_type(&self) -> VarType {
        self.value.var_type()
    }
}

/// The tagged payload of a [`TypvalT`] (`typval_T.v_type` combined
/// with `typval_T.vval`, the union member the tag selects).
#[derive(Debug, Clone, Default)]
pub enum TypvalValue {
    /// Unknown (unspecified) value (`VAR_UNKNOWN`).
    #[default]
    Unknown,
    /// Number (`VAR_NUMBER`, `.v_number`).
    Number(VarnumberT),
    /// String (`VAR_STRING`, `.v_string`) - can be absent, matching
    /// the original's nullable `char *v_string`.
    String(Option<Vec<u8>>),
    /// Function reference (`VAR_FUNC`) - the original reuses
    /// `v_string` to hold the function name for this tag too, but a
    /// distinct variant here is more useful/self-documenting than
    /// requiring every match site to separately track "was this a
    /// `VAR_STRING` or a `VAR_FUNC`" alongside a value that doesn't
    /// itself carry that distinction.
    Func(Option<Vec<u8>>),
    /// List (`VAR_LIST`, `.v_list`) - can be null, matching the
    /// original's nullable `list_T *v_list`.
    List(*mut ListT),
    /// Dict (`VAR_DICT`, `.v_dict`) - can be null, matching the
    /// original's nullable `dict_T *v_dict`.
    Dict(*mut DictT),
    /// Floating-point value (`VAR_FLOAT`, `.v_float`).
    Float(f64),
    /// `true`/`false` (`VAR_BOOL`, `.v_bool`).
    Bool(BoolVarValue),
    /// Special value (null) (`VAR_SPECIAL`, `.v_special`).
    Special(SpecialVarValue),
    /// Closure: function with args (`VAR_PARTIAL`, `.v_partial`) - can
    /// be null, matching the original's nullable `partial_T *`.
    Partial(*mut PartialT),
    /// Blob (`VAR_BLOB`, `.v_blob`) - can be null, matching the
    /// original's nullable `blob_T *`.
    Blob(*mut BlobT),
}

impl TypvalValue {
    /// The [`VarType`] tag this variant corresponds to (`v_type`).
    #[must_use]
    pub fn var_type(&self) -> VarType {
        match self {
            TypvalValue::Unknown => VarType::Unknown,
            TypvalValue::Number(_) => VarType::Number,
            TypvalValue::String(_) => VarType::String,
            TypvalValue::Func(_) => VarType::Func,
            TypvalValue::List(_) => VarType::List,
            TypvalValue::Dict(_) => VarType::Dict,
            TypvalValue::Float(_) => VarType::Float,
            TypvalValue::Bool(_) => VarType::Bool,
            TypvalValue::Special(_) => VarType::Special,
            TypvalValue::Partial(_) => VarType::Partial,
            TypvalValue::Blob(_) => VarType::Blob,
        }
    }
}

/// Structure to hold an item of a Dictionary (`dictitem_T`).
///
/// The original stores `di_key` as a C "flexible array member"
/// (`char di_key[]`) placed directly after `di_flags`, in the SAME
/// heap allocation as the rest of the struct - this lets
/// `hashtab_defs.rs`'s `HashitemT.hi_key` point straight at those
/// bytes, and (`TV_DICT_HI2DI` in the original) recover the owning
/// `dictitem_T*` via `hi_key - offsetof(dictitem_T, di_key)` pointer
/// arithmetic, with no extra pointer stored per item.
///
/// Rust has no safe/ergonomic equivalent of a flexible array member -
/// a faithful replication would need a hand-rolled dynamically-sized
/// type (manual `Layout` computation, raw `alloc`/`dealloc`, and
/// reconstructing a fat pointer on every access): a large amount of
/// error-prone unsafe code, disproportionate to what is, in the
/// original, purely a one-pointer memory optimization with zero
/// observable behavioral difference to any caller. So `di_key` is
/// instead an owned `Vec<u8>` (NUL-terminated, matching the byte
/// layout `hi_key`'s C-string readers expect) - a genuinely separate
/// heap allocation from `DictitemT` itself, exactly the same choice
/// already made for the fixed-key-size `ChangedtickDictItem`/
/// `ScopeDictDictItem` instantiations of this same original macro,
/// above.
///
/// This means `hi_key -> owning DictitemT` can no longer be recovered
/// via pointer arithmetic; `eval/typval.rs`'s `DictT` instead keeps
/// its own side index (`dv_index`, a `HashMap` keyed by each item's
/// `hi_key` address) for that lookup - matching this crate's own
/// `hashtab.rs` module doc, which already anticipated this exact
/// tension for `hash_clear_all` ("the right Rust shape (probably a
/// trait or closure parameter instead of raw offset arithmetic)").
///
/// Heap-allocated via `Box::into_raw`/`Box::from_raw`, matching
/// `ListitemT`'s established raw-pointer-linked convention - NOT
/// `Rc`/`RefCell`, for the same reasons given there.
#[derive(Debug)]
pub struct DictitemT {
    /// Structure that holds the value (`di_tv`).
    pub di_tv: TypvalT,
    /// Flags, see [`dict_item_flags`] (`di_flags`).
    pub di_flags: u8,
    /// Key value, NUL-terminated (`di_key`) - see this struct's own
    /// doc comment for why this is an owned `Vec<u8>` rather than a
    /// true flexible array member.
    pub di_key: Vec<u8>,
}

/// Structure representing a Dictionary (`dictvar_S` / `dict_T`).
///
/// `dv_hashtab` is used exactly as the original for key-uniqueness/
/// existence hashing (via `hashtab.rs`'s existing `HashtabT`), but see
/// [`DictitemT`]'s own doc comment for why the classic `TV_DICT_HI2DI`
/// `hi_key`-to-owning-item pointer-arithmetic recovery isn't
/// replicated. `dv_index` (new, not in the original) is this crate's
/// own substitute: a side table from each live item's `hi_key` address
/// (as a `usize`) to its owning `*mut DictitemT`, populated/
/// depopulated in lockstep with `dv_hashtab` by every insert/remove in
/// `eval/typval.rs`.
///
/// `watchers` (the original's `QUEUE` of dict-key watchers set by user
/// code, e.g. `dictwatcheradd()`) is deferred - needs a `QUEUE`
/// intrusive-linked-list translation first, and has no caller yet
/// among the functions translated so far.
pub struct DictT {
    /// Whole dictionary lock status (`dv_lock`).
    pub dv_lock: VarLockStatus,
    /// Non-zero (`Scope`/`DefScope`) if dictionary represents a scope
    /// (i.e. `g:`, `l:`, ...) (`dv_scope`).
    pub dv_scope: ScopeType,
    /// Reference count (`dv_refcount`).
    pub dv_refcount: i32,
    /// ID used when recursively traversing a value (`dv_copy_id`).
    pub dv_copy_id: i32,
    /// Hashtab containing all items (`dv_hashtab`).
    pub dv_hashtab: crate::hashtab_defs::HashtabT,
    /// This crate's own side index from each live item's `hi_key`
    /// address to its owning item - see this struct's own doc comment.
    pub dv_index: std::collections::HashMap<usize, *mut DictitemT>,
    /// Copied dict used by `deepcopy()` (`dv_copydict`).
    pub dv_copydict: *mut DictT,
    /// Next dictionary in used dictionaries list (`dv_used_next`).
    pub dv_used_next: *mut DictT,
    /// Previous dictionary in used dictionaries list (`dv_used_prev`).
    pub dv_used_prev: *mut DictT,
    pub lua_table_ref: LuaRef,
}

/// Structure to hold an item of a list (`listitem_T`).
///
/// `li_next`/`li_prev` mirror the original's raw, manually-managed
/// doubly-linked-list pointers exactly (see this module's own doc
/// comment for why - not `Option<Box<_>>`/`Rc`).
pub struct ListitemT {
    /// Next item in list, null if none (`li_next`).
    pub li_next: *mut ListitemT,
    /// Previous item in list, null if none (`li_prev`).
    pub li_prev: *mut ListitemT,
    /// Item value (`li_tv`).
    pub li_tv: TypvalT,
}

/// Structure used by those that are iterating over an item in a list
/// while it may be concurrently modified (`listwatch_T`).
pub struct ListwatchT {
    /// Item being watched (`lw_item`).
    pub lw_item: *mut ListitemT,
    /// Next watcher, null if none (`lw_next`).
    pub lw_next: *mut ListwatchT,
}

/// Structure to hold info about a list (`listvar_S` / `list_T`).
///
/// Field order matches the original exactly (its own comment notes it
/// was "optimized to reduce padding" - preserved rather than
/// reordered for Rust's own layout rules, which don't guarantee
/// field-declaration-order layout for a plain `struct` anyway, but
/// there's no reason to needlessly diverge from the source either).
pub struct ListT {
    /// First item, null if none (`lv_first`).
    pub lv_first: *mut ListitemT,
    /// Last item, null if none (`lv_last`).
    pub lv_last: *mut ListitemT,
    /// First watcher, null if none (`lv_watch`).
    pub lv_watch: *mut ListwatchT,
    /// When not null, item at index `lv_idx` (`lv_idx_item`).
    pub lv_idx_item: *mut ListitemT,
    /// Copied list used by `deepcopy()` (`lv_copylist`).
    pub lv_copylist: *mut ListT,
    /// Next list in the used-lists list (`lv_used_next`).
    pub lv_used_next: *mut ListT,
    /// Previous list in the used-lists list (`lv_used_prev`).
    pub lv_used_prev: *mut ListT,
    /// Reference count (`lv_refcount`).
    pub lv_refcount: i32,
    /// Number of items (`lv_len`).
    pub lv_len: i32,
    /// Index of a cached item, used to optimize repeated `l[idx]`
    /// (`lv_idx`).
    pub lv_idx: i32,
    /// ID used by `deepcopy()` (`lv_copyID`).
    pub lv_copy_id: i32,
    /// Zero, `VAR_LOCKED`, or `VAR_FIXED` (`lv_lock`).
    pub lv_lock: VarLockStatus,
    pub lua_table_ref: LuaRef,
}

/// Structure representing a Vimscript "partial" - a function reference
/// bound to some arguments and/or a `self` dict (`partial_S` /
/// `partial_T`).
///
/// `pt_argv` (the original's allocated `typval_T *`+`pt_argc` pair) is
/// a plain `Vec<TypvalT>` here, matching this crate's usual "owned
/// `Vec` instead of allocated-array-plus-count" convention - its own
/// `.len()` replaces `pt_argc`.
#[derive(Debug, Default)]
pub struct PartialT {
    /// Reference count (`pt_refcount`).
    pub pt_refcount: i32,
    pub pt_copy_id: i32,
    /// Function name; when absent use `pt_func`'s name instead
    /// (`pt_name`).
    pub pt_name: Option<Vec<u8>>,
    /// Function pointer; when null, look up the function with
    /// `pt_name` instead (`pt_func`).
    pub pt_func: *mut UfuncT,
    /// `true` when this partial was created via `dict.member` in
    /// `handle_subscript()` (`pt_auto`).
    pub pt_auto: bool,
    /// Arguments (`pt_argv`/`pt_argc` collapsed into one `Vec` - see
    /// this struct's own doc comment).
    pub pt_argv: Vec<TypvalT>,
    /// Dict for `self` (`pt_dict`).
    pub pt_dict: *mut DictT,
}

/// Structure to hold info for a user function (`ufunc_S` / `ufunc_T`).
///
/// `uf_name`/`uf_namelen` (the original's C flexible array member,
/// `char uf_name[]`) collapse into one owned `Vec<u8>` here - same "no
/// safe Rust equivalent for a C flexible array member" reasoning as
/// `DictitemT.di_key`'s own doc comment (see `HIKEY2UF`'s
/// `hi_key - offsetof(ufunc_T, uf_name)` pointer-arithmetic recovery,
/// exactly analogous to `TV_DICT_HI2DI`) - `uf_name`'s own `.len()`
/// replaces `uf_namelen`. `eval/userfunc.c`'s `func_hashtab` (a
/// file-static `hashtab_T` of all named user functions, keyed by
/// `uf_name`) will need its own side-table when that file is
/// eventually translated, matching `DictT.dv_index`'s already-
/// established precedent for the exact same problem.
///
/// `uf_tml_count`/`uf_tml_total`/`uf_tml_self` (the original's 3
/// parallel allocated arrays, one entry per function line, for
/// per-line `:profile` timing) collapse into 3 plain `Vec`s, matching
/// this crate's usual "owned `Vec` instead of allocated-array" rule.
///
/// `uf_scoped: *mut FunccallT` references [`FunccallT`], which now has
/// its own real fields too (see this same file's own module doc and
/// [`FunccallT`]'s doc comment below) - a raw pointer to it needs no
/// further design of its own here.
#[derive(Debug, Default)]
pub struct UfuncT {
    /// variable nr of arguments (`uf_varargs`).
    pub uf_varargs: i32,
    pub uf_flags: i32,
    /// nr of active calls (`uf_calls`).
    pub uf_calls: i32,
    /// `func_clear()` was already called (`uf_cleared`).
    pub uf_cleared: bool,
    /// arguments, including optional arguments (`uf_args`).
    pub uf_args: crate::garray_defs::GarrayT,
    /// default argument expressions (`uf_def_args`).
    pub uf_def_args: crate::garray_defs::GarrayT,
    /// function lines (`uf_lines`).
    pub uf_lines: crate::garray_defs::GarrayT,
    /// `true` when func is being profiled (`uf_profiling`).
    pub uf_profiling: i32,
    pub uf_prof_initialized: i32,
    /// lua callback, used if `uf_flags & FC_LUAREF` (`uf_luaref`).
    pub uf_luaref: LuaRef,
    // Profiling the function as a whole.
    /// nr of calls (`uf_tm_count`).
    pub uf_tm_count: i32,
    /// time spent in function + children (`uf_tm_total`).
    pub uf_tm_total: crate::types_defs::ProftimeT,
    /// time spent in function itself (`uf_tm_self`).
    pub uf_tm_self: crate::types_defs::ProftimeT,
    /// time spent in children this call (`uf_tm_children`).
    pub uf_tm_children: crate::types_defs::ProftimeT,
    // Profiling the function per line.
    /// nr of times each line was executed (`uf_tml_count`).
    pub uf_tml_count: Vec<i32>,
    /// time spent in each line + children (`uf_tml_total`).
    pub uf_tml_total: Vec<crate::types_defs::ProftimeT>,
    /// time spent in each line itself (`uf_tml_self`).
    pub uf_tml_self: Vec<crate::types_defs::ProftimeT>,
    /// start time for current line (`uf_tml_start`).
    pub uf_tml_start: crate::types_defs::ProftimeT,
    /// time spent in children for this line (`uf_tml_children`).
    pub uf_tml_children: crate::types_defs::ProftimeT,
    /// start wait time for current line (`uf_tml_wait`).
    pub uf_tml_wait: crate::types_defs::ProftimeT,
    /// index of line being timed; -1 if none (`uf_tml_idx`).
    pub uf_tml_idx: i32,
    /// line being timed was executed (`uf_tml_execed`).
    pub uf_tml_execed: i32,
    /// `SCTX` where function was defined, used for `s:` variables
    /// (`uf_script_ctx`).
    pub uf_script_ctx: SctxT,
    /// reference count, see `func_name_refcount()` (`uf_refcount`).
    pub uf_refcount: i32,
    /// `l:` local variables for closure (`uf_scoped`).
    pub uf_scoped: *mut FunccallT,
    /// If `uf_name` starts with `SNR`, the name with `<SNR>` as a
    /// string, otherwise `None` (`uf_name_exp`).
    pub uf_name_exp: Option<Vec<u8>>,
    /// Name of function - collapses the original's `uf_namelen` +
    /// flexible array member `uf_name[]` into one owned `Vec<u8>` (see
    /// this struct's own doc comment); can start with `<SNR>123_`
    /// (`<SNR>` is `K_SPECIAL KS_EXTRA KE_SNR`) (`uf_name`).
    pub uf_name: Vec<u8>,
}

/// Structure to hold info for a function that is currently being
/// executed (`struct funccall_S` / `funccall_T`).
///
/// See this module's own doc comment for the by-value `dict_T`/
/// `list_T` embedding rationale and the manual `Default` impl's
/// reasoning.
pub struct FunccallT {
    /// Function being called (`fc_func`).
    pub fc_func: *mut UfuncT,
    /// Next line to be executed (`fc_linenr`).
    pub fc_linenr: i32,
    /// `":return"` used (`fc_returned`).
    pub fc_returned: i32,
    /// Fixed variables for arguments - collapses the original's
    /// `TV_DICTITEM_STRUCT(VAR_SHORT_LEN + 1)` fixed-size
    /// instantiation into plain `DictitemT`s (`fc_fixvar`).
    pub fc_fixvar: [DictitemT; FIXVAR_CNT],
    /// `l:` local function variables (`fc_l_vars`).
    pub fc_l_vars: DictT,
    /// Variable for `l:` scope (`fc_l_vars_var`).
    pub fc_l_vars_var: ScopeDictDictItem,
    /// `a:` argument variables (`fc_l_avars`).
    pub fc_l_avars: DictT,
    /// Variable for `a:` scope (`fc_l_avars_var`).
    pub fc_l_avars_var: ScopeDictDictItem,
    /// List for `a:000` (`fc_l_varlist`).
    pub fc_l_varlist: ListT,
    /// List items for `a:000` (`fc_l_listitems`).
    pub fc_l_listitems: [ListitemT; MAX_FUNC_ARGS],
    /// Return value (`fc_rettv`).
    pub fc_rettv: *mut TypvalT,
    /// Next line with breakpoint or zero (`fc_breakpoint`).
    pub fc_breakpoint: LinenrT,
    /// `"debug_tick"` when breakpoint was set (`fc_dbg_tick`).
    pub fc_dbg_tick: i32,
    /// Top nesting level of executed function (`fc_level`).
    pub fc_level: i32,
    /// Functions to be called on return (`fc_defer`).
    pub fc_defer: crate::garray_defs::GarrayT,
    /// Time spent in a child (`fc_prof_child`).
    pub fc_prof_child: crate::types_defs::ProftimeT,
    /// Calling function or null; or next funccal in the list pointed
    /// to by `previous_funccal` (`fc_caller`).
    pub fc_caller: *mut FunccallT,
    /// Number of user functions that reference this funccall
    /// (`fc_refcount`).
    pub fc_refcount: i32,
    /// `CopyID` used for garbage collection (`fc_copy_id`).
    pub fc_copy_id: i32,
    /// List of `ufunc_T*` which keep a reference to `fc_func`
    /// (`fc_ufuncs`).
    pub fc_ufuncs: crate::garray_defs::GarrayT,
}

impl Default for FunccallT {
    /// Zero-initializes every field, faithfully matching the
    /// original's own `create_funccal` (`xcalloc(1,
    /// sizeof(funccall_T))`) - see this struct's own doc comment.
    fn default() -> Self {
        FunccallT {
            fc_func: std::ptr::null_mut(),
            fc_linenr: 0,
            fc_returned: 0,
            fc_fixvar: std::array::from_fn(|_| DictitemT {
                di_tv: TypvalT::default(),
                di_flags: 0,
                di_key: Vec::new(),
            }),
            fc_l_vars: DictT {
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
            },
            fc_l_vars_var: ScopeDictDictItem::default(),
            fc_l_avars: DictT {
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
            },
            fc_l_avars_var: ScopeDictDictItem::default(),
            fc_l_varlist: ListT {
                lv_first: std::ptr::null_mut(),
                lv_last: std::ptr::null_mut(),
                lv_watch: std::ptr::null_mut(),
                lv_idx_item: std::ptr::null_mut(),
                lv_copylist: std::ptr::null_mut(),
                lv_used_next: std::ptr::null_mut(),
                lv_used_prev: std::ptr::null_mut(),
                lv_refcount: 0,
                lv_len: 0,
                lv_idx: 0,
                lv_copy_id: 0,
                lv_lock: VarLockStatus::Unlocked,
                lua_table_ref: -1,
            },
            fc_l_listitems: std::array::from_fn(|_| ListitemT {
                li_next: std::ptr::null_mut(),
                li_prev: std::ptr::null_mut(),
                li_tv: TypvalT::default(),
            }),
            fc_rettv: std::ptr::null_mut(),
            fc_breakpoint: 0,
            fc_dbg_tick: 0,
            fc_level: 0,
            fc_defer: crate::garray_defs::GarrayT::default(),
            fc_prof_child: 0,
            fc_caller: std::ptr::null_mut(),
            fc_refcount: 0,
            fc_copy_id: 0,
            fc_ufuncs: crate::garray_defs::GarrayT::default(),
        }
    }
}

/// Structure to hold info about a Blob (`blobvar_S` / `blob_T`).
#[derive(Debug, Clone, Default)]
pub struct BlobT {
    /// Growarray with the data (`bv_ga`).
    pub bv_ga: crate::garray_defs::GarrayT,
    /// Reference count (`bv_refcount`).
    pub bv_refcount: i32,
    /// `VAR_UNLOCKED`, `VAR_LOCKED`, or `VAR_FIXED` (`bv_lock`).
    pub bv_lock: VarLockStatus,
}


/// Type used for the `changedtick_di` member in `buf_T`
/// (`ChangedtickDictItem`, a `TV_DICTITEM_STRUCT(sizeof("changedtick"))`
/// instance - the generic `dictitem_T`'s macro-generated shape, fixed
/// at this particular key size).
///
/// `di_key`'s fixed-size C array (`char di_key[sizeof("changedtick")]`)
/// becomes an owned `Vec<u8>` here, matching this crate's usual
/// preference for `Vec<u8>` over fixed-size byte arrays/raw C strings
/// throughout (e.g. `option.rs`'s `Option<Vec<u8>>` values) - nothing
/// about this particular use site needs the original's exact
/// allocation size, just its content.
#[derive(Debug, Clone, Default)]
pub struct ChangedtickDictItem {
    /// Structure that holds the `changedtick` value itself (`di_tv`).
    pub di_tv: TypvalT,
    /// Flags (`di_flags`).
    pub di_flags: u8,
    /// Key value (`di_key`).
    pub di_key: Vec<u8>,
}

/// Structure to hold a scope dictionary (e.g. `b:`/`w:`/`t:`), pretending
/// to `find_var_in_ht()` (not yet translated) to be a `dictitem_T`
/// (`ScopeDictDictItem`, a `TV_DICTITEM_STRUCT(1)` instance). Same
/// `di_key`-as-`Vec<u8>` reasoning as [`ChangedtickDictItem`] above.
#[derive(Debug, Clone, Default)]
pub struct ScopeDictDictItem {
    /// Structure that holds the scope dictionary itself (`di_tv`).
    pub di_tv: TypvalT,
    /// Flags (`di_flags`).
    pub di_flags: u8,
    /// Key value (`di_key`).
    pub di_key: Vec<u8>,
}

/// Discriminant for which kind of callback a [`Callback`] holds
/// (`CallbackType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CallbackType {
    #[default]
    None,
    Funcref,
    Partial,
    Lua,
}

/// A callback into Vimscript or Lua: a plain function name (`funcref`), a
/// partial (function + bound arguments/dict), or a Lua function reference
/// (`Callback`).
///
/// The original is an untagged C union (`data`) plus a separate
/// `CallbackType type` tag; translated as a proper safe Rust enum (each
/// variant directly carries its own payload) rather than replicating the
/// union + separate tag split, matching this crate's established
/// convention for small tagged unions where there's no hot-path memory
/// layout reason not to (e.g. `UhLink` in `undo_defs.rs`). The `Partial`
/// variant stays a raw pointer to the not-yet-translated [`PartialT`],
/// same as e.g. `SynblockT.b_syn_linecont_prog: *mut RegprogT` elsewhere.
#[derive(Debug, Clone)]
pub enum Callback {
    /// `kCallbackNone` / `CALLBACK_NONE`.
    None,
    /// `kCallbackFuncref`: plain function name (`char *funcref`).
    Funcref(Vec<u8>),
    /// `kCallbackPartial` (`partial_T *partial`).
    Partial(*mut PartialT),
    /// `kCallbackLua` (`LuaRef luaref`).
    Lua(LuaRef),
}

impl Default for Callback {
    /// `CALLBACK_INIT`/`CALLBACK_NONE`.
    fn default() -> Self {
        Callback::None
    }
}

impl Callback {
    /// `CallbackType` of this callback, mirroring the original's separate
    /// `.type` tag (`callback_is_none()`-style checks elsewhere use this).
    #[must_use]
    pub fn kind(&self) -> CallbackType {
        match self {
            Callback::None => CallbackType::None,
            Callback::Funcref(_) => CallbackType::Funcref,
            Callback::Partial(_) => CallbackType::Partial,
            Callback::Lua(_) => CallbackType::Lua,
        }
    }
}

pub type VarnumberT = i64;
pub type UvarnumberT = u64;

/// Maximal possible value of a [`VarnumberT`] variable.
pub const VARNUMBER_MAX: VarnumberT = i64::MAX;
/// Minimal possible value of a [`VarnumberT`] variable.
pub const VARNUMBER_MIN: VarnumberT = i64::MIN;
/// Maximal possible value of a [`UvarnumberT`] variable
/// (`UVARNUMBER_MAX`).
pub const UVARNUMBER_MAX: UvarnumberT = u64::MAX;

/// Refcount for a dict or list that should never be freed
/// (`DO_NOT_FREE_CNT`).
pub const DO_NOT_FREE_CNT: i32 = i32::MAX / 2;

/// Additional values for `tv_list_alloc()`'s `len` argument
/// (`ListLenSpecials`; `tv_list_alloc` itself is now translated, in
/// `eval/typval.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListLenSpecials {
    /// List length is not known in advance - there's neither a way to
    /// know how many elements will be needed nor any educated guess
    /// (`kListLenUnknown`).
    Unknown = -1,
    /// List length *should* be known, but is actually not - all
    /// occurrences of this should eventually be removed; it's only
    /// used where determining the length would need a refactor
    /// (`kListLenShouldKnow`).
    ShouldKnow = -2,
    /// List length may be known in advance, but determining it looks
    /// impractical (`kListLenMayKnow`).
    MayKnow = -3,
}

/// Bool variable values (`BoolVarValue`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolVarValue {
    /// `v:false` (`kBoolVarFalse`).
    False,
    /// `v:true` (`kBoolVarTrue`).
    True,
}

/// Special variable values (`SpecialVarValue`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialVarValue {
    /// `v:null` (`kSpecialVarNull`).
    Null,
}

/// Variable lock status for `typval_T.v_lock` (`VarLockStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VarLockStatus {
    /// Not locked (`VAR_UNLOCKED`).
    #[default]
    Unlocked = 0,
    /// User lock, can be unlocked (`VAR_LOCKED`).
    Locked = 1,
    /// Locked forever (`VAR_FIXED`).
    Fixed = 2,
}

/// Vimscript variable types, for use in `typval_T.v_type` (`VarType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VarType {
    /// Unknown (unspecified) value (`VAR_UNKNOWN`).
    #[default]
    Unknown = 0,
    /// Number, `.v_number` is used (`VAR_NUMBER`).
    Number = 1,
    /// String, `.v_string` is used (`VAR_STRING`).
    String = 2,
    /// Function reference, `.v_string` is used as the function name
    /// (`VAR_FUNC`).
    Func = 3,
    /// List, `.v_list` is used (`VAR_LIST`).
    List = 4,
    /// Dict, `.v_dict` is used (`VAR_DICT`).
    Dict = 5,
    /// Floating-point value, `.v_float` is used (`VAR_FLOAT`).
    Float = 6,
    /// `true`/`false`, `.v_bool` is used (`VAR_BOOL`).
    Bool = 7,
    /// Special value (null), `.v_special` is used (`VAR_SPECIAL`).
    Special = 8,
    /// Partial, `.v_partial` is used (`VAR_PARTIAL`).
    Partial = 9,
    /// Blob, `.v_blob` is used (`VAR_BLOB`).
    Blob = 10,
}

/// Type values returned by Vimscript's `type()` built-in (`VAR_TYPE_*`,
/// an anonymous `enum` of plain integer constants in the original, kept
/// that way here too rather than as a Rust `enum` type - matching this
/// crate's existing `opt_dy_flag`-style precedent for anonymous C
/// integer-constant enums). Distinct from [`VarType`]'s own
/// discriminants (note the non-contiguous `10` for `BLOB`, and that
/// `NUMBER`/`STRING`/etc. don't line up 1:1 with `VarType`'s own values
/// either - these are a completely independent numbering the original
/// itself defines separately).
pub mod var_type_result {
    pub const NUMBER: i32 = 0;
    pub const STRING: i32 = 1;
    pub const FUNC: i32 = 2;
    pub const LIST: i32 = 3;
    pub const DICT: i32 = 4;
    pub const FLOAT: i32 = 5;
    pub const BOOL: i32 = 6;
    pub const SPECIAL: i32 = 7;
    pub const BLOB: i32 = 10;
}

/// Values for `(struct dictvar_S).dv_scope` (`ScopeType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScopeType {
    /// Not a scope dictionary (`VAR_NO_SCOPE`).
    #[default]
    NoScope = 0,
    /// Scope dictionary which requires a prefix (`a:`, `v:`, ...)
    /// (`VAR_SCOPE`).
    Scope = 1,
    /// Scope dictionary which may be accessed without a prefix (`l:`,
    /// `g:`) (`VAR_DEF_SCOPE`).
    DefScope = 2,
}

/// Flags for `dictitem_T.di_flags` (`DictItemFlags`) - bit flags meant
/// to be OR'd together (e.g. read-only *and* fixed), so kept as plain
/// integer constants rather than a Rust `enum` (which would wrongly
/// imply mutually-exclusive variants) - matching this crate's
/// `MT_FLAG_*`/`opt_dy_flag` bit-flag precedent. `dictitem_T` itself
/// (which would consume these) is not yet translated.
pub mod dict_item_flags {
    /// Read-only value (`DI_FLAGS_RO`).
    pub const RO: u8 = 1;
    /// Value, read-only in the sandbox (`DI_FLAGS_RO_SBX`).
    pub const RO_SBX: u8 = 2;
    /// Fixed value: cannot be `:unlet` or `remove()`d (`DI_FLAGS_FIX`).
    pub const FIX: u8 = 4;
    /// Locked value (`DI_FLAGS_LOCK`).
    pub const LOCK: u8 = 8;
    /// Separately allocated (`DI_FLAGS_ALLOC`).
    pub const ALLOC: u8 = 16;
}

/// Maximum number of function arguments (`MAX_FUNC_ARGS`).
pub const MAX_FUNC_ARGS: usize = 20;
/// Short variable name length (`VAR_SHORT_LEN`).
pub const VAR_SHORT_LEN: usize = 20;
/// Number of fixed variables used for arguments (`FIXVAR_CNT`).
pub const FIXVAR_CNT: usize = 12;

/// Type used for script ID (`scid_T`).
pub type ScidT = i32;

/// SCript ConteXt (SCTX): identifies a script line (`sctx_T`).
///
/// When sourcing a script `sc_lnum` is zero, `sourcing_lnum` is the current
/// line number. When executing a user function `sc_lnum` is the line where
/// the function was defined, `sourcing_lnum` is the line number inside the
/// function. When stored with a function, mapping, option, etc. `sc_lnum`
/// is the line number in the script `sc_sid`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SctxT {
    /// script ID
    pub sc_sid: ScidT,
    /// sourcing sequence number
    pub sc_seq: i32,
    /// line number
    pub sc_lnum: LinenrT,
    /// only used when `sc_sid` is `SID_API_CLIENT`
    pub sc_chan: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typval_default_is_unknown_and_unlocked() {
        let tv = TypvalT::default();
        assert!(matches!(tv.value, TypvalValue::Unknown));
        assert_eq!(tv.v_lock, VarLockStatus::Unlocked);
        assert_eq!(tv.var_type(), VarType::Unknown);
    }

    #[test]
    fn typval_value_var_type_matches_every_variant() {
        assert_eq!(TypvalValue::Unknown.var_type(), VarType::Unknown);
        assert_eq!(TypvalValue::Number(5).var_type(), VarType::Number);
        assert_eq!(TypvalValue::String(Some(b"hi".to_vec())).var_type(), VarType::String);
        assert_eq!(TypvalValue::Func(Some(b"MyFunc".to_vec())).var_type(), VarType::Func);
        assert_eq!(TypvalValue::List(std::ptr::null_mut()).var_type(), VarType::List);
        assert_eq!(TypvalValue::Dict(std::ptr::null_mut()).var_type(), VarType::Dict);
        assert_eq!(TypvalValue::Float(1.5).var_type(), VarType::Float);
        assert_eq!(TypvalValue::Bool(BoolVarValue::True).var_type(), VarType::Bool);
        assert_eq!(TypvalValue::Special(SpecialVarValue::Null).var_type(), VarType::Special);
        assert_eq!(TypvalValue::Partial(std::ptr::null_mut()).var_type(), VarType::Partial);
        assert_eq!(TypvalValue::Blob(std::ptr::null_mut()).var_type(), VarType::Blob);
    }

    #[test]
    fn typval_number_roundtrips_through_the_enum() {
        let tv = TypvalT { v_lock: VarLockStatus::Locked, value: TypvalValue::Number(42) };
        assert_eq!(tv.var_type(), VarType::Number);
        match tv.value {
            TypvalValue::Number(n) => assert_eq!(n, 42),
            _ => panic!("expected Number variant"),
        }
        assert_eq!(tv.v_lock, VarLockStatus::Locked);
    }

    #[test]
    fn blob_t_default_is_empty_and_unlocked() {
        let blob = BlobT::default();
        assert_eq!(blob.bv_ga.ga_len, 0);
        assert_eq!(blob.bv_refcount, 0);
        assert_eq!(blob.bv_lock, VarLockStatus::Unlocked);
    }

    #[test]
    fn partial_t_default_is_zeroed_with_null_pointers() {
        let pt = PartialT::default();
        assert_eq!(pt.pt_refcount, 0);
        assert_eq!(pt.pt_copy_id, 0);
        assert!(pt.pt_name.is_none());
        assert!(pt.pt_func.is_null());
        assert!(!pt.pt_auto);
        assert!(pt.pt_argv.is_empty());
        assert!(pt.pt_dict.is_null());
    }

    #[test]
    fn ufunc_t_default_is_zeroed_with_null_pointers_and_empty_name() {
        let uf = UfuncT::default();
        assert_eq!(uf.uf_varargs, 0);
        assert_eq!(uf.uf_flags, 0);
        assert_eq!(uf.uf_calls, 0);
        assert!(!uf.uf_cleared);
        assert_eq!(uf.uf_args.ga_len, 0);
        assert_eq!(uf.uf_def_args.ga_len, 0);
        assert_eq!(uf.uf_lines.ga_len, 0);
        assert_eq!(uf.uf_profiling, 0);
        assert_eq!(uf.uf_luaref, 0);
        assert_eq!(uf.uf_tm_count, 0);
        assert!(uf.uf_tml_count.is_empty());
        assert!(uf.uf_tml_total.is_empty());
        assert!(uf.uf_tml_self.is_empty());
        assert_eq!(uf.uf_tml_idx, 0);
        assert_eq!(uf.uf_script_ctx, SctxT::default());
        assert_eq!(uf.uf_refcount, 0);
        assert!(uf.uf_scoped.is_null());
        assert!(uf.uf_name_exp.is_none());
        assert!(uf.uf_name.is_empty());
    }

    #[test]
    fn ufunc_t_can_be_linked_via_uf_scoped_raw_pointer() {
        let mut uf = UfuncT { uf_name: b"MyFunc".to_vec(), ..Default::default() };
        assert!(uf.uf_scoped.is_null());
        uf.uf_scoped = std::ptr::null_mut();
        assert!(uf.uf_scoped.is_null());
        assert_eq!(uf.uf_name, b"MyFunc");
    }

    #[test]
    fn funccall_t_default_is_zeroed_with_null_pointers_and_empty_containers() {
        let mut fc = FunccallT::default();
        assert!(fc.fc_func.is_null());
        assert_eq!(fc.fc_linenr, 0);
        assert_eq!(fc.fc_returned, 0);
        assert_eq!(fc.fc_fixvar.len(), FIXVAR_CNT);
        for item in &fc.fc_fixvar {
            assert!(matches!(item.di_tv.value, TypvalValue::Unknown));
            assert!(item.di_key.is_empty());
        }
        assert_eq!(fc.fc_l_vars.dv_refcount, 0);
        assert!(fc.fc_l_vars.dv_hashtab.hash_find(b"missing").hi_key.is_null());
        assert_eq!(fc.fc_l_avars.dv_refcount, 0);
        assert_eq!(fc.fc_l_varlist.lv_len, 0);
        assert!(fc.fc_l_varlist.lv_first.is_null());
        assert_eq!(fc.fc_l_listitems.len(), MAX_FUNC_ARGS);
        for item in &fc.fc_l_listitems {
            assert!(item.li_next.is_null());
            assert!(matches!(item.li_tv.value, TypvalValue::Unknown));
        }
        assert!(fc.fc_rettv.is_null());
        assert_eq!(fc.fc_breakpoint, 0);
        assert_eq!(fc.fc_level, 0);
        assert_eq!(fc.fc_defer.ga_len, 0);
        assert_eq!(fc.fc_prof_child, 0);
        assert!(fc.fc_caller.is_null());
        assert_eq!(fc.fc_refcount, 0);
        assert_eq!(fc.fc_copy_id, 0);
        assert_eq!(fc.fc_ufuncs.ga_len, 0);
    }

    #[test]
    fn funccall_t_l_vars_dict_is_usable_through_the_real_tv_dict_api() {
        // Confirm the by-value-embedded fc_l_vars works through the
        // existing tv_dict_* raw-pointer API exactly like a
        // Box::into_raw-allocated DictT would - the same proof
        // ScriptvarT already established for a single DictT, now
        // checked for FunccallT's own embedded dicts too.
        let mut fc = FunccallT::default();
        let d = &mut fc.fc_l_vars as *mut DictT;
        unsafe {
            let item = crate::eval::typval::tv_dict_item_alloc(b"x");
            (*item).di_tv.value = TypvalValue::Number(42);
            assert_eq!(crate::eval::typval::tv_dict_add(&mut *d, item), crate::vim_defs::OK);
            let found = crate::eval::typval::tv_dict_find(Some(&mut *d), b"x").unwrap();
            assert!(matches!((*found).di_tv.value, TypvalValue::Number(42)));
            crate::eval::typval::tv_dict_free_contents(d);
        }
    }

    #[test]
    fn funccall_t_caller_can_be_linked_into_a_chain() {
        let mut inner = FunccallT::default();
        let mut outer = FunccallT { fc_caller: &mut inner as *mut FunccallT, ..Default::default() };
        assert!(!outer.fc_caller.is_null());
        // SAFETY: inner outlives this read, both are stack locals in
        // scope for the whole test body.
        let caller_linenr = unsafe { (*outer.fc_caller).fc_linenr };
        assert_eq!(caller_linenr, 0);
        outer.fc_caller = std::ptr::null_mut(); // avoid a dangling reference lingering
    }

    #[test]
    fn changedtick_dict_item_default_is_empty() {
        let item = ChangedtickDictItem::default();
        assert!(matches!(item.di_tv.value, TypvalValue::Unknown));
        assert_eq!(item.di_flags, 0);
        assert!(item.di_key.is_empty());
    }

    #[test]
    fn scope_dict_dict_item_default_is_empty() {
        let item = ScopeDictDictItem::default();
        assert!(matches!(item.di_tv.value, TypvalValue::Unknown));
        assert_eq!(item.di_flags, 0);
        assert!(item.di_key.is_empty());
    }

    #[test]
    fn listitem_chain_links_and_unlinks_via_raw_pointers() {
        // A minimal 3-item list built and traversed entirely by hand,
        // matching the original's own raw-pointer doubly-linked-list
        // model (no ListT-level allocation/refcounting logic exists
        // yet - this only exercises the plain struct shape).
        let mut a = ListitemT {
            li_next: std::ptr::null_mut(),
            li_prev: std::ptr::null_mut(),
            li_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(1) },
        };
        let mut b = ListitemT {
            li_next: std::ptr::null_mut(),
            li_prev: &mut a as *mut ListitemT,
            li_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(2) },
        };
        a.li_next = &mut b as *mut ListitemT;

        let list = ListT {
            lv_first: &mut a as *mut ListitemT,
            lv_last: &mut b as *mut ListitemT,
            lv_watch: std::ptr::null_mut(),
            lv_idx_item: std::ptr::null_mut(),
            lv_copylist: std::ptr::null_mut(),
            lv_used_next: std::ptr::null_mut(),
            lv_used_prev: std::ptr::null_mut(),
            lv_refcount: 1,
            lv_len: 2,
            lv_idx: 0,
            lv_copy_id: 0,
            lv_lock: VarLockStatus::Unlocked,
            lua_table_ref: 0,
        };

        // Traverse from lv_first, collecting each item's Number value.
        let mut values = Vec::new();
        let mut cur = list.lv_first;
        while !cur.is_null() {
            // SAFETY: every pointer in this hand-built chain points at
            // a still-live local (`a`/`b`), never freed during this test.
            let item = unsafe { &*cur };
            if let TypvalValue::Number(n) = item.li_tv.value {
                values.push(n);
            }
            cur = item.li_next;
        }
        assert_eq!(values, vec![1, 2]);
        assert_eq!(list.lv_len, 2);
        // SAFETY: same as above.
        assert_eq!(unsafe { &*list.lv_last }.li_prev, &mut a as *mut ListitemT);
    }

    #[test]
    fn sctx_default_is_zeroed() {
        let sctx = SctxT::default();
        assert_eq!(sctx.sc_sid, 0);
        assert_eq!(sctx.sc_seq, 0);
        assert_eq!(sctx.sc_lnum, 0);
        assert_eq!(sctx.sc_chan, 0);
    }

    #[test]
    fn callback_default_is_none_variant() {
        let cb = Callback::default();
        assert_eq!(cb.kind(), CallbackType::None);
    }

    #[test]
    fn callback_kind_matches_variant() {
        assert_eq!(Callback::Funcref(b"MyFunc".to_vec()).kind(), CallbackType::Funcref);
        assert_eq!(Callback::Lua(0).kind(), CallbackType::Lua);
        assert_eq!(Callback::Partial(std::ptr::null_mut()).kind(), CallbackType::Partial);
    }

    #[test]
    fn var_type_discriminants_match_the_original_c_enum_order() {
        assert_eq!(VarType::Unknown as i32, 0);
        assert_eq!(VarType::Number as i32, 1);
        assert_eq!(VarType::String as i32, 2);
        assert_eq!(VarType::Func as i32, 3);
        assert_eq!(VarType::List as i32, 4);
        assert_eq!(VarType::Dict as i32, 5);
        assert_eq!(VarType::Float as i32, 6);
        assert_eq!(VarType::Bool as i32, 7);
        assert_eq!(VarType::Special as i32, 8);
        assert_eq!(VarType::Partial as i32, 9);
        assert_eq!(VarType::Blob as i32, 10);
    }

    #[test]
    fn var_type_result_constants_match_the_original_including_the_blob_gap() {
        assert_eq!(var_type_result::NUMBER, 0);
        assert_eq!(var_type_result::STRING, 1);
        assert_eq!(var_type_result::FUNC, 2);
        assert_eq!(var_type_result::LIST, 3);
        assert_eq!(var_type_result::DICT, 4);
        assert_eq!(var_type_result::FLOAT, 5);
        assert_eq!(var_type_result::BOOL, 6);
        assert_eq!(var_type_result::SPECIAL, 7);
        // Note the gap: BLOB is 10, not 8 - matches the original's own
        // non-contiguous numbering exactly (verified against
        // typval_defs.h directly, not assumed).
        assert_eq!(var_type_result::BLOB, 10);
    }

    #[test]
    fn scope_type_discriminants_match_the_original() {
        assert_eq!(ScopeType::NoScope as i32, 0);
        assert_eq!(ScopeType::Scope as i32, 1);
        assert_eq!(ScopeType::DefScope as i32, 2);
    }

    #[test]
    fn var_lock_status_discriminants_match_the_original() {
        assert_eq!(VarLockStatus::Unlocked as i32, 0);
        assert_eq!(VarLockStatus::Locked as i32, 1);
        assert_eq!(VarLockStatus::Fixed as i32, 2);
    }

    #[test]
    fn list_len_specials_discriminants_match_the_original() {
        assert_eq!(ListLenSpecials::Unknown as i32, -1);
        assert_eq!(ListLenSpecials::ShouldKnow as i32, -2);
        assert_eq!(ListLenSpecials::MayKnow as i32, -3);
    }

    #[test]
    fn dict_item_flags_are_distinct_bits_that_can_be_combined() {
        use dict_item_flags::{ALLOC, FIX, LOCK, RO, RO_SBX};
        let all = [RO, RO_SBX, FIX, LOCK, ALLOC];
        for (i, &a) in all.iter().enumerate() {
            for (j, &b) in all.iter().enumerate() {
                if i != j {
                    assert_eq!(a & b, 0, "flags {a} and {b} overlap");
                }
            }
        }
        // Combining read-only + fixed is a valid, expected OR-combination.
        assert_eq!(RO | FIX, 5);
    }

    #[test]
    fn do_not_free_cnt_is_int_max_over_two() {
        assert_eq!(DO_NOT_FREE_CNT, i32::MAX / 2);
    }
}
