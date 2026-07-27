//! Translated from `src/nvim/eval/funcs.c` (tractable core only).
//!
//! `funcs.c` (~7700 lines) implements EVERY builtin Vimscript function
//! (`len()`, `type()`, `split()`, `map()`, `sort()`, and roughly 640
//! more) plus the generated (`funcs.generated.h`) name/argument-count
//! dispatch table (`functions[]`) that `find_internal_func`/
//! `call_internal_func` search. Translating all of it is a separate,
//! massive undertaking of its own - this file starts that undertaking
//! with the dispatch mechanism itself plus a small handful of the
//! simplest, most foundational functions, to be extended incrementally
//! (matching how `eval.rs`'s own `eval0`-`eval7` chain grew).
//!
//! Translated: [`EvalFuncDefT`] (`EvalFuncDef`), [`find_internal_func`]/
//! [`call_internal_func`], and a growing set of the simplest builtins:
//! `len()`, `type()`, `empty()` - each already expressible in terms of
//! already-translated helpers ([`crate::eval::typval::tv_get_string`]/
//! `tv_list_len`/`tv_dict_len`/`tv_blob_len`) - plus a second batch:
//! the three bitwise functions `and()`/`or()`/`xor()`, `abs()`,
//! `max()`/`min()` (shared `max_min` helper, mirroring the original's
//! own structure exactly), the character/number conversions
//! `char2nr()`/`nr2char()`, and the string-to-number/float conversions
//! `str2nr()`/`str2float()` (the former, unusually, lives in
//! `strings.c` in the original rather than `funcs.c` itself - noted on
//! `f_str2nr`'s own doc comment) - plus a third batch: `str2list()`/
//! `list2str()` (character-codepoint `List` <-> `String` conversion;
//! `str2list` lives in `strings.c`, `list2str` in `eval/typval.c` -
//! neither in `funcs.c` itself, also noted on their own doc comments).
//! `list2str`/`str2list` needed a new [`crate::eval::typval::tv_list_alloc_ret`]
//! (allocate a list directly into a `rettv`, used pervasively - 83
//! call sites - by the real, untranslated `funcs.c`, so translated as
//! its own genuine function rather than inlined for this one caller).
//!
//! The real `functions[]` table is machine-generated from every
//! `f_*` function's own doc comment (`gen_eval.lua` scanning for
//! `@function`-tagged comments) into a perfect hash
//! (`find_internal_func_hash`); this crate uses a plain
//! `HashMap<&'static [u8], EvalFuncDefT>` instead - same data, a
//! different but functionally-identical lookup mechanism, matching
//! `option.rs`'s own `OPTION_HASH_ELEMS` precedent for exactly the
//! same "the original's own dispatch tree is a pure C performance
//! micro-optimization, not business logic" reasoning.
//!
//! `base_arg` (which argument position, if any, a function can be
//! called on via `expr->name()` method-call syntax) is stored for
//! future fidelity but not yet acted on - `call_internal_method`
//! (`handle_subscript`'s own "->name()" real caller) isn't translated
//! yet either.
//!
//! Also translated: a full cluster of `float_op_wrapper`-style pure-
//! float builtins - `sin()`/`cos()`/`tan()`/`asin()`/`acos()`/`atan()`/
//! `sinh()`/`cosh()`/`tanh()`/`sqrt()`/`exp()`/`log()`/`log10()`/
//! `floor()`/`ceil()`/`round()`/`trunc()` (single-argument), `atan2()`/
//! `pow()`/`fmod()` (two-argument), and `float2nr()`. The original
//! dispatches ALL of these through ONE shared C function
//! (`float_op_wrapper`), selected per-entry via an `EvalFuncData
//! .func_float` function pointer stored directly in the generated
//! `functions[]` table itself - contrary to this module's own earlier
//! doc comment (since corrected), this does NOT actually require
//! widening [`EvalFuncDefT`]/[`VimLFuncT`] at all: each `f_sin`/
//! `f_cos`/etc. below is instead its own tiny function that already
//! "bakes in" which specific `f64` method to call, delegating to a
//! shared private `float_op_wrapper`/`float_op2_wrapper` helper pair
//! that only need a plain function-pointer parameter - same data (a
//! name maps to a specific float transform), a different but
//! functionally-identical mechanism, matching this module's own
//! already-established `FUNCTIONS`-table-vs-perfect-hash precedent.
//! Needed a new [`crate::eval::typval::tv_get_float_chk`] (the
//! original's own `tv_get_float_chk`, an explicit-success/failure
//! sibling of the already-translated [`crate::eval::typval::tv_get_float`]).
//!
//! Also translated: `tolower()`/`toupper()` (via the already-existing
//! `crate::strings::strcase_save`) and `trim()` (leading/trailing
//! character trimming with an optional mask and direction, via a
//! by-hand backward-multibyte-walk using the already-existing
//! `crate::mbyte::utf_head_off`). `strcase_save`'s own trailing NUL
//! terminator (added for its own, differently-shaped C-string-flavored
//! callers) is stripped before building this crate's own `String`
//! typval. `utf_head_off` requires its own `base` argument to include
//! a trailing NUL byte - `f_trim` builds one once, up front, rather
//! than threading that requirement through its own established
//! "embedded NUL ends a C-string-modeled scan" byte-length-bounded
//! idiom used everywhere else in this module.
//!
//! Also translated: `has_key()` (via the already-existing
//! [`crate::eval::typval::tv_dict_has_key`]) and `keys()`/`values()`/
//! `items()` (a shared private `tv_dict2list` helper, mirroring the
//! original's own `tv_dict2list`/`DictListType` split exactly). Only
//! `items()`'s `Dict` case is translated - the original also accepts a
//! `String`/`List`/`Blob` via `tv_string2items`/`tv_list2items`/
//! `tv_blob2items`, none of which exist yet, so `f_items` declines
//! those cases explicitly. A dict item's own `di_key` always carries a
//! trailing NUL terminator (`DictitemT`'s own C-string-compatible
//! storage convention) that must be stripped before use as a
//! Vimscript `String` value - caught by a real test failure (asserting
//! the exact string CONTENTS, not just the list's own length) during
//! this function's own development, fixed using the same established
//! stripping idiom `eval/typval.rs`'s own `tv_dict_equal` already uses.
//!
//! Also translated: `get()` (a generic getter for `Blob`/`List`/
//! `Dict` with an optional default for a missing/out-of-range entry).
//! Only those three container types are handled - the original also
//! accepts a `Funcref`/`Partial` (returning `"func"`/`"name"`/`"dict"`/
//! `"args"`/`"arity"` introspection sub-fields via `get_func_arity`,
//! not yet translated), which `f_get` declines explicitly.
//!
//! Also translated: `index()` (the index of the first `Blob`/`List`
//! item equal to a given value, via the already-existing
//! [`crate::eval::typval::tv_equal`]).
//!
//! Also translated: `reverse()` (`Blob`/`List`/`String` in place),
//! needing a new [`crate::eval::typval::tv_blob_set`] (`eval/typval.h`'s
//! own `static inline` byte-store counterpart to the already-
//! translated `tv_blob_get`) and a new private `reverse_text` (a
//! UTF-8-aware, non-destructive character reversal using the already-
//! existing `crate::mbyte::utfc_ptr2len`).
//!
//! Also translated: `count()` (occurrences of a value in a `String`/
//! `List`/`Dict`, via three private helpers - `count_string`/
//! `count_list`/`count_dict` - mirroring the original's own identical
//! split exactly).
//!
//! Also translated: `copy()` (a shallow copy of `List`/`Dict`/`Blob`/
//! any other value, via [`crate::eval::typval::tv_list_copy`]/
//! [`crate::eval::typval::tv_dict_copy`]/[`crate::eval::typval::tv_blob_copy`]),
//! and `deepcopy()` (a RECURSIVE copy, via `eval.c`'s own
//! [`crate::eval::eval::var_item_copy`] plus real `deep=true` support
//! added to `tv_list_copy`/`tv_dict_copy` - cycle detection via an
//! optional `copy_id`/[`crate::eval::eval::get_copy_id`] is fully
//! real too, matching the original's own documented `{noref}`
//! semantics exactly: a container referenced more than once is only
//! copied once by default, or once per occurrence with
//! `deepcopy({expr}, 1)`).
//!
//! Also translated (from `eval/list.c`, not `funcs.c` itself):
//! `add()` (append one item to a `List`/`Blob`, returning the SAME
//! container) and `insert()` (insert one item before a given index,
//! or at the start by default, also returning the SAME container).
//! Also `remove()` (delete an item/range from a `List`/`Blob`, or a
//! key from a `Dict`), which needed 2 new `typval.rs` helpers of its
//! own: [`crate::eval::typval::tv_list_move_items`] (`eval/typval.c`'s
//! own function, moving a range of items from one list to the end of
//! another) and the whole `tv_list_remove`/`tv_blob_remove`/
//! `tv_dict_remove` trio (also `eval/typval.c`, one real function per
//! container type, exactly matching the original's own 3-way split).
//! Also `extend()`/`extendnew()` (merge one `List`/`Dict` into
//! another, in place or into a new copy), which needed a new
//! [`crate::eval::typval::tv_dict_extend`] (`eval/typval.c`) - its
//! `action="move"` case (moving items rather than copying) panics if
//! actually reached (needs a dict-item detach-without-free primitive
//! this crate doesn't have yet), but is provably unreachable from
//! `extend()`/`extendnew()` themselves (their own 3rd-argument
//! validation never allows `"move"` - the original's only `"move"`
//! caller is `window.c`'s scroll-event handling, not yet translated).
//!
//! Also `range()` (a `List` of numbers, optionally strided/counting
//! down).
//!
//! Also `repeat()` (repeat a `String`/`List`/`Blob` `{count}` times,
//! concatenated) - its `Blob` case builds the repeated buffer
//! directly via a plain `Vec<u8>` rather than the original's own
//! `ga_grow`+`tv_blob_set_range`-based approach (including a "skip
//! the copy if already all zero" micro-optimization not needed here).
//!
//! Also `join()` (from `eval/typval.c`, not `funcs.c` itself), which
//! needed a new [`crate::eval::typval::tv_list_join`] - fully real
//! for `Number`/`Float`/`String`/`Bool`/`Special` items (the
//! overwhelmingly common real-world case), but panics if an item is
//! `List`/`Dict`/`Blob`/`Funcref`/`Partial`-typed - stringifying THOSE
//! needs the full `encode_tv2echo` machinery (`eval/encode.c`, ~970
//! lines, a substantial separate undertaking not attempted here).
//!
//! Also `flatten()`/`flattennew()` (recursively replace each nested
//! `List` item, in place or into a new copy, up to an optional
//! `{maxdepth}`), which needed a new
//! [`crate::eval::typval::tv_list_flatten`] (`eval/typval.c`).
//!
//! Also `localtime()` (the current Unix timestamp, via the already-
//! existing [`crate::os::time::os_time`]).
//!
//! Also `getenv()` (via the already-existing
//! [`crate::os::env::vim_getenv`]) and `environ()` - the latter has NO
//! C implementation at all in the original (it's implemented in Lua,
//! `runtime/lua/vim/_core/vimfn.lua`'s own `M.f_environ`, calling
//! `vim.uv.os_environ()` and force-uppercasing keys on Windows only) -
//! translated directly from that Lua source using
//! `std::env::vars_os()` as the portable equivalent, matching this
//! whole mission's own scope (Neovim's Lua source, not just its C
//! source).
//!
//! Also `has({feature})` (`f_has`): checks `HAS_LIST_UNCONDITIONAL`
//! (the original's own `has_list[]` static array's platform-
//! independent entries - compile-time feature flags, not runtime
//! state) plus the handful of platform-conditional (`#ifdef`) entries
//! this crate can meaningfully determine at compile time. Every other
//! special case the original handles dynamically (`patch-N`/
//! `nvim-x.y.z` version checks, `vim_starting`/`ttyin`/`ttyout`/
//! `gui_running`/`syntax_items`/`wsl` runtime state, and provider-
//! based checks like `clipboard_working`) simply returns `0` - a
//! deliberate, justified scoping decision (not a "translate the
//! reachable path, panic on the rest" case like most other builtins):
//! `has()`'s own contract is "is this feature present", and "not
//! found" is a fully legitimate, non-crashing answer within that
//! contract, unlike e.g. `join()` where a specific argument type has
//! one well-defined correct stringification that must not be silently
//! wrong.
//!
//! Also a batch of `strings.c`-hosted string-inspection/extraction
//! builtins: `strlen()` (byte length), `strcharlen()`/`strchars()`
//! (character count, composing-aware via the new
//! [`crate::mbyte::mb_ptr2char_adv`]/[`crate::mbyte::mb_cptr2char_adv`]),
//! `strwidth()` (display cells, via the new
//! [`crate::mbyte::mb_string2cells`]), `stridx()`/`strridx()`
//! (forward/reverse substring search), `strgetchar()` (character at a
//! character index), `strpart()` (byte or, with `{chars}`, character-
//! counted substring extraction), and `strtrans()` (unprintable-
//! character escaping, via the already-translated
//! [`crate::charset::transstr`]).
//!
//! Also `byteidx()`/`byteidxcomp()` (the byte index of the Nth
//! character, composing folded-in vs. separate), `charidx()` (inverse:
//! character index of a given byte), `strcharpart()` (like `strpart()`
//! but character- rather than byte-counted), `getpid()` (the
//! process ID, via the already-existing
//! [`crate::os::env::os_get_pid`]), `tr()` (position-based character
//! translation, `strings.c`), and `hostname()` (another Lua-
//! implemented builtin like `environ()` - `func_lua = 'f_hostname'` in
//! `eval.lua` - via the already-existing
//! [`crate::os::env::os_get_hostname`]). `isdirectory()`/
//! `isabsolutepath()`/`delete()`/`pathshorten()` (originating from
//! `eval/fs.c`/`path.c`) live in their own mirrored files
//! ([`crate::eval::fs`]/[`crate::path`]) but are registered in this
//! module's own `FUNCTIONS` table.
//!
//! Also 5 more small, self-contained builtins: `foreground()` (a true
//! no-op even in the original - empty function body), `eventhandler()`
//! (via the already-real `GLOBALS.vgetc_busy`), `did_filetype()` (via
//! the already-real `BufT.b_did_filetype`), `garbagecollect()` (sets
//! the already-real `GLOBALS.want_garbage_collect`/
//! `garbage_collect_at_exit` flags faithfully - the actual collection
//! pass itself is deferred to the toplevel execution loop in the
//! original too, not yet translated here either), and
//! `getcharsearch()` (via the already-existing
//! [`crate::search::last_csearch_str`]/`last_csearch_forward`/
//! `last_csearch_until`).
//!
//! Also `mode([{expr}])`, via the already-translated
//! [`crate::state::get_mode`] (re-investigated: `state.c`'s `get_mode`
//! is a pure state-to-string formatter, NOT genuinely event-loop-bound
//! like `state_enter` - see `get_mode`'s own doc comment for exactly
//! which of its branches are decidable today vs. still unreachable).
//!
//! Also `visualmode([{expr}])` (via the already-real
//! `BufT.b_visual_mode_eval`), `wildmenumode()` (via the already-real
//! `GLOBALS.wild_menu_showing`, short-circuiting away its own
//! `MODE_CMDLINE`-dependent second operand exactly like the original
//! does, since nothing can currently set that `State` bit), and
//! `windowsversion()` (via the already-real
//! `GLOBALS.windowsVersion`, zero-initialized/empty until `main.c`'s
//! own version-detection code runs - not yet translated, so this
//! matches the original's real "non-MS-Windows" behavior exactly on
//! every platform today, not an approximation).
//!
//! Also `getreg([{regname} [, {expr} [, {list}]]])`, via the already-
//! existing [`crate::register::get_reg_contents`] (defaults to
//! `v:register` when `{regname}` is omitted). The `{list}`-truthy
//! path panics inside `get_reg_contents` itself, matching that
//! function's own already-documented `kGRegList` deferral.

use crate::eval::typval_defs::{TypvalT, TypvalValue};
use crate::eval::userfunc::FnameTransError;

/// A builtin function can't be used as a method at all (`BASE_NONE`).
pub const BASE_NONE: u8 = 0;
/// A builtin function's LAST argument is the method base (`BASE_LAST`).
pub const BASE_LAST: u8 = u8::MAX;

/// A builtin Vimscript function's own implementation signature
/// (`VimLFunc`).
///
/// # Safety
/// If any `argvars[i].value`/`rettv.value` (on entry) is
/// `List`/`Dict`/`Blob`/`Partial`-typed with a non-null pointer, that
/// pointer must be valid, matching every other function in this crate
/// touching those types.
pub type VimLFuncT = unsafe fn(argvars: &[TypvalT], rettv: &mut TypvalT);

/// A builtin Vimscript function's own definition (`EvalFuncDef`).
#[derive(Clone, Copy)]
pub struct EvalFuncDefT {
    /// Minimal number of arguments (`min_argc`).
    pub min_argc: u8,
    /// Maximal number of arguments (`max_argc`).
    pub max_argc: u8,
    /// Method base arg # (1-indexed), [`BASE_NONE`] or [`BASE_LAST`]
    /// (`base_arg`).
    pub base_arg: u8,
    /// Function implementation (`func`).
    pub func: VimLFuncT,
}

/// Every translated builtin function name mapped to its own
/// definition (`functions[]`, mechanically a tiny subset of the real
/// ~641-entry table - see this module's own doc comment).
static FUNCTIONS: std::sync::LazyLock<crate::globals::GlobalCell<std::collections::HashMap<&'static [u8], EvalFuncDefT>>> =
    std::sync::LazyLock::new(|| {
        let mut m = std::collections::HashMap::new();
        m.insert(&b"len"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_len });
        m.insert(&b"type"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_type });
        m.insert(&b"empty"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_empty });
        m.insert(&b"and"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_and });
        m.insert(&b"or"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_or });
        m.insert(&b"xor"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_xor });
        m.insert(&b"abs"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_abs });
        m.insert(&b"max"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_max });
        m.insert(&b"min"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_min });
        m.insert(&b"char2nr"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: f_char2nr });
        m.insert(&b"nr2char"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: f_nr2char });
        m.insert(&b"str2float"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_str2float });
        m.insert(&b"str2nr"[..], EvalFuncDefT { min_argc: 1, max_argc: 3, base_arg: 1, func: f_str2nr });
        m.insert(&b"str2list"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: f_str2list });
        m.insert(&b"list2str"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: f_list2str });
        m.insert(&b"sin"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_sin });
        m.insert(&b"cos"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_cos });
        m.insert(&b"tan"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_tan });
        m.insert(&b"asin"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_asin });
        m.insert(&b"acos"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_acos });
        m.insert(&b"atan"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_atan });
        m.insert(&b"sinh"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_sinh });
        m.insert(&b"cosh"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_cosh });
        m.insert(&b"tanh"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_tanh });
        m.insert(&b"sqrt"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_sqrt });
        m.insert(&b"exp"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_exp });
        m.insert(&b"log"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_log });
        m.insert(&b"log10"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_log10 });
        m.insert(&b"floor"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_floor });
        m.insert(&b"ceil"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_ceil });
        m.insert(&b"round"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_round });
        m.insert(&b"trunc"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_trunc });
        m.insert(&b"atan2"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_atan2 });
        m.insert(&b"pow"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_pow });
        m.insert(&b"fmod"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_fmod });
        m.insert(&b"float2nr"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_float2nr });
        m.insert(&b"tolower"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_tolower });
        m.insert(&b"toupper"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_toupper });
        m.insert(&b"trim"[..], EvalFuncDefT { min_argc: 1, max_argc: 3, base_arg: 1, func: f_trim });
        m.insert(&b"has_key"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_has_key });
        m.insert(&b"keys"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_keys });
        m.insert(&b"values"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_values });
        m.insert(&b"items"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_items });
        m.insert(&b"get"[..], EvalFuncDefT { min_argc: 2, max_argc: 3, base_arg: 1, func: f_get });
        m.insert(&b"index"[..], EvalFuncDefT { min_argc: 2, max_argc: 4, base_arg: 1, func: f_index });
        m.insert(&b"reverse"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_reverse });
        m.insert(&b"count"[..], EvalFuncDefT { min_argc: 2, max_argc: 4, base_arg: 1, func: f_count });
        m.insert(&b"copy"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_copy });
        m.insert(&b"deepcopy"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: f_deepcopy });
        m.insert(&b"add"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_add });
        m.insert(&b"insert"[..], EvalFuncDefT { min_argc: 2, max_argc: 3, base_arg: 1, func: f_insert });
        m.insert(&b"remove"[..], EvalFuncDefT { min_argc: 2, max_argc: 3, base_arg: 1, func: f_remove });
        m.insert(&b"extend"[..], EvalFuncDefT { min_argc: 2, max_argc: 3, base_arg: 1, func: f_extend });
        m.insert(&b"extendnew"[..], EvalFuncDefT { min_argc: 2, max_argc: 3, base_arg: 1, func: f_extendnew });
        m.insert(&b"range"[..], EvalFuncDefT { min_argc: 1, max_argc: 3, base_arg: 1, func: f_range });
        m.insert(&b"repeat"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_repeat });
        m.insert(&b"join"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: f_join });
        m.insert(&b"flatten"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: f_flatten });
        m.insert(&b"flattennew"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: f_flattennew });
        m.insert(&b"localtime"[..], EvalFuncDefT { min_argc: 0, max_argc: 0, base_arg: BASE_NONE, func: f_localtime });
        m.insert(&b"getenv"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_getenv });
        m.insert(&b"environ"[..], EvalFuncDefT { min_argc: 0, max_argc: 0, base_arg: BASE_NONE, func: f_environ });
        m.insert(&b"setenv"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 2, func: f_setenv });
        m.insert(&b"has"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_has });
        m.insert(&b"strlen"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_strlen });
        m.insert(&b"strcharlen"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_strcharlen });
        m.insert(&b"strchars"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: f_strchars });
        m.insert(&b"strwidth"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_strwidth });
        m.insert(&b"stridx"[..], EvalFuncDefT { min_argc: 2, max_argc: 3, base_arg: 1, func: f_stridx });
        m.insert(&b"strridx"[..], EvalFuncDefT { min_argc: 2, max_argc: 3, base_arg: 1, func: f_strridx });
        m.insert(&b"strgetchar"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_strgetchar });
        m.insert(&b"strpart"[..], EvalFuncDefT { min_argc: 2, max_argc: 4, base_arg: 1, func: f_strpart });
        m.insert(&b"strtrans"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_strtrans });
        m.insert(&b"byteidx"[..], EvalFuncDefT { min_argc: 2, max_argc: 3, base_arg: 1, func: f_byteidx });
        m.insert(&b"byteidxcomp"[..], EvalFuncDefT { min_argc: 2, max_argc: 3, base_arg: 1, func: f_byteidxcomp });
        m.insert(&b"charidx"[..], EvalFuncDefT { min_argc: 2, max_argc: 4, base_arg: 1, func: f_charidx });
        m.insert(&b"strcharpart"[..], EvalFuncDefT { min_argc: 2, max_argc: 4, base_arg: 1, func: f_strcharpart });
        m.insert(&b"getpid"[..], EvalFuncDefT { min_argc: 0, max_argc: 0, base_arg: BASE_NONE, func: f_getpid });
        m.insert(&b"tr"[..], EvalFuncDefT { min_argc: 3, max_argc: 3, base_arg: 1, func: f_tr });
        m.insert(&b"isdirectory"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: crate::eval::fs::f_isdirectory });
        m.insert(&b"isabsolutepath"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: crate::eval::fs::f_isabsolutepath });
        m.insert(&b"delete"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: crate::eval::fs::f_delete });
        m.insert(&b"filereadable"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: crate::eval::fs::f_filereadable });
        m.insert(&b"filewritable"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: crate::eval::fs::f_filewritable });
        m.insert(&b"getfsize"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: crate::eval::fs::f_getfsize });
        m.insert(&b"getftime"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: crate::eval::fs::f_getftime });
        m.insert(&b"getftype"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: crate::eval::fs::f_getftype });
        m.insert(&b"pathshorten"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: crate::eval::fs::f_pathshorten });
        m.insert(&b"hostname"[..], EvalFuncDefT { min_argc: 0, max_argc: 0, base_arg: BASE_NONE, func: f_hostname });
        m.insert(&b"foreground"[..], EvalFuncDefT { min_argc: 0, max_argc: 0, base_arg: BASE_NONE, func: f_foreground });
        m.insert(&b"eventhandler"[..], EvalFuncDefT { min_argc: 0, max_argc: 0, base_arg: BASE_NONE, func: f_eventhandler });
        m.insert(&b"did_filetype"[..], EvalFuncDefT { min_argc: 0, max_argc: 0, base_arg: BASE_NONE, func: f_did_filetype });
        m.insert(&b"garbagecollect"[..], EvalFuncDefT { min_argc: 0, max_argc: 1, base_arg: BASE_NONE, func: f_garbagecollect });
        m.insert(&b"getcharsearch"[..], EvalFuncDefT { min_argc: 0, max_argc: 0, base_arg: BASE_NONE, func: f_getcharsearch });
        m.insert(&b"mode"[..], EvalFuncDefT { min_argc: 0, max_argc: 1, base_arg: 1, func: f_mode });
        m.insert(&b"visualmode"[..], EvalFuncDefT { min_argc: 0, max_argc: 1, base_arg: BASE_NONE, func: f_visualmode });
        m.insert(&b"wildmenumode"[..], EvalFuncDefT { min_argc: 0, max_argc: 0, base_arg: BASE_NONE, func: f_wildmenumode });
        m.insert(&b"windowsversion"[..], EvalFuncDefT { min_argc: 0, max_argc: 0, base_arg: BASE_NONE, func: f_windowsversion });
        m.insert(&b"getreg"[..], EvalFuncDefT { min_argc: 0, max_argc: 3, base_arg: 1, func: f_getreg });
        m.insert(&b"changenr"[..], EvalFuncDefT { min_argc: 0, max_argc: 0, base_arg: BASE_NONE, func: f_changenr });
        m.insert(&b"interrupt"[..], EvalFuncDefT { min_argc: 0, max_argc: 0, base_arg: BASE_NONE, func: f_interrupt });
        m.insert(&b"invert"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_invert });
        m.insert(&b"getfontname"[..], EvalFuncDefT { min_argc: 0, max_argc: 1, base_arg: BASE_NONE, func: f_getfontname });
        m.insert(&b"isinf"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_isinf });
        m.insert(&b"isnan"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_isnan });
        m.insert(&b"sha256"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_sha256 });
        m.insert(&b"exists"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_exists });
        m.insert(&b"getwinpos"[..], EvalFuncDefT { min_argc: 0, max_argc: 1, base_arg: 1, func: f_getwinpos });
        m.insert(&b"getwinposx"[..], EvalFuncDefT { min_argc: 0, max_argc: 0, base_arg: BASE_NONE, func: f_getwinposx });
        m.insert(&b"getwinposy"[..], EvalFuncDefT { min_argc: 0, max_argc: 0, base_arg: BASE_NONE, func: f_getwinposy });
        m.insert(&b"win_getid"[..], EvalFuncDefT { min_argc: 0, max_argc: 2, base_arg: 1, func: f_win_getid });
        m.insert(&b"win_id2win"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_win_id2win });
        m.insert(&b"win_id2tabwin"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_win_id2tabwin });
        m.insert(&b"win_findbuf"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_win_findbuf });
        m.insert(&b"winnr"[..], EvalFuncDefT { min_argc: 0, max_argc: 1, base_arg: 1, func: f_winnr });
        crate::globals::GlobalCell::new(m)
    });

/// Find a builtin function's own definition by name (`find_internal_func`).
#[must_use]
pub fn find_internal_func(name: &[u8]) -> Option<EvalFuncDefT> {
    // SAFETY: no overlapping live access - see this crate's established
    // GlobalCell::get_mut convention.
    unsafe { FUNCTIONS.get_mut() }.get(name).copied()
}

/// Call a builtin function by name (`call_internal_func`).
///
/// # Safety
/// Forwarded from the dispatched-to [`VimLFuncT`]'s own safety doc.
#[must_use]
pub unsafe fn call_internal_func(fname: &[u8], argvars: &[TypvalT], rettv: &mut TypvalT) -> FnameTransError {
    let Some(fdef) = find_internal_func(fname) else {
        return FnameTransError::Unknown;
    };
    if argvars.len() < fdef.min_argc as usize {
        return FnameTransError::TooFew;
    }
    if argvars.len() > fdef.max_argc as usize {
        return FnameTransError::TooMany;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (fdef.func)(argvars, rettv) };
    FnameTransError::None
}

/// `len({expr})` - length of a string/number/`Blob`/`List`/`Dict`
/// (`f_len`).
///
/// The original's `E701: Invalid type for len()` error (for a
/// `Float`/`Bool`/`Special`/`Partial`/`Func`/`Unknown` argument) is
/// omitted - message display, not tractable; `rettv` is simply left
/// untouched (still its caller's own default-initialized `Number(0)`),
/// matching this crate's established "skip the display, keep an
/// otherwise-harmless default" policy for similar leaf-level gaps.
///
/// # Safety
/// If `argvars[0].value` is `List`/`Dict`/`Blob`-typed with a non-null
/// pointer, that pointer must be valid.
unsafe fn f_len(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(match &argvars[0].value {
        TypvalValue::String(_) | TypvalValue::Number(_) => {
            crate::eval::typval::tv_get_string(&argvars[0]).len() as crate::eval::typval_defs::VarnumberT
        }
        // SAFETY: forwarded from this function's own safety doc.
        TypvalValue::Blob(b) => crate::eval::typval_defs::VarnumberT::from(unsafe { crate::eval::typval::tv_blob_len(*b) }),
        // SAFETY: forwarded from this function's own safety doc.
        TypvalValue::List(l) => crate::eval::typval_defs::VarnumberT::from(unsafe { crate::eval::typval::tv_list_len(*l) }),
        // SAFETY: forwarded from this function's own safety doc.
        TypvalValue::Dict(d) => {
            crate::eval::typval_defs::VarnumberT::from(crate::eval::typval::tv_dict_len(unsafe { d.as_ref() }))
        }
        _ => return,
    });
}

/// `type({expr})` - a number identifying `{expr}`'s own type
/// (`f_type`). The returned numbers are fixed, documented,
/// externally-observable values (`v:t_*` constants), not internal
/// implementation details.
fn f_type(argvars: &[TypvalT], rettv: &mut TypvalT) {
    use crate::eval::typval_defs::var_type_result;
    let n = match &argvars[0].value {
        TypvalValue::Number(_) => var_type_result::NUMBER,
        TypvalValue::String(_) => var_type_result::STRING,
        TypvalValue::Partial(_) | TypvalValue::Func(_) => var_type_result::FUNC,
        TypvalValue::List(_) => var_type_result::LIST,
        TypvalValue::Dict(_) => var_type_result::DICT,
        TypvalValue::Float(_) => var_type_result::FLOAT,
        TypvalValue::Bool(_) => var_type_result::BOOL,
        TypvalValue::Special(_) => var_type_result::SPECIAL,
        TypvalValue::Blob(_) => var_type_result::BLOB,
        TypvalValue::Unknown => {
            debug_assert!(false, "f_type(UNKNOWN) - internal_error in the original");
            -1
        }
    };
    rettv.value = TypvalValue::Number(crate::eval::typval_defs::VarnumberT::from(n));
}

/// `empty({expr})` - whether `{expr}` is "empty" (`f_empty`).
///
/// # Safety
/// If `argvars[0].value` is `List`/`Dict`/`Partial`-typed with a
/// non-null pointer, that pointer must be valid.
unsafe fn f_empty(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let n = match &argvars[0].value {
        TypvalValue::String(s) => s.as_deref().is_none_or(<[u8]>::is_empty),
        TypvalValue::Func(s) => s.as_deref().is_none_or(<[u8]>::is_empty),
        TypvalValue::Partial(p) => p.is_null(),
        TypvalValue::Number(n) => *n == 0,
        TypvalValue::Float(f) => *f == 0.0,
        // SAFETY: forwarded from this function's own safety doc.
        TypvalValue::List(l) => (unsafe { crate::eval::typval::tv_list_len(*l) }) == 0,
        TypvalValue::Dict(d) => crate::eval::typval::tv_dict_len(unsafe { d.as_ref() }) == 0,
        TypvalValue::Bool(b) => *b == crate::eval::typval_defs::BoolVarValue::False,
        // v:null is the only real SpecialVarValue variant in this
        // crate (v:true/v:false moved to the separate BoolVarValue
        // type upstream too) - always "empty", matching the original's
        // own `n = argvars[0].vval.v_special == kSpecialVarNull`.
        TypvalValue::Special(s) => *s == crate::eval::typval_defs::SpecialVarValue::Null,
        TypvalValue::Blob(b) => {
            // SAFETY: forwarded from this function's own safety doc.
            (unsafe { crate::eval::typval::tv_blob_len(*b) }) == 0
        }
        TypvalValue::Unknown => {
            debug_assert!(false, "f_empty(UNKNOWN) - internal_error in the original");
            true
        }
    };
    // Unlike a "real" Vimscript Bool, empty() has always returned a
    // plain Number (0/1) for historical compatibility - it relies on
    // call_func's own "default rettv is number zero" pre-initialization
    // in the original (only ever overwriting `.vval.v_number`, never
    // touching `.v_type`); this crate's own TypvalValue enum has no
    // such split-tag/union hazard, so this is just a direct, explicit
    // Number assignment instead.
    rettv.value = TypvalValue::Number(crate::eval::typval_defs::VarnumberT::from(n));
}

/// `and({expr}, {expr})` - bitwise AND of two numbers (`f_and`).
fn f_and(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let a = crate::eval::typval::tv_get_number_chk(&argvars[0], None);
    let b = crate::eval::typval::tv_get_number_chk(&argvars[1], None);
    rettv.value = TypvalValue::Number(a & b);
}

/// `or({expr}, {expr})` - bitwise OR of two numbers (`f_or`).
fn f_or(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let a = crate::eval::typval::tv_get_number_chk(&argvars[0], None);
    let b = crate::eval::typval::tv_get_number_chk(&argvars[1], None);
    rettv.value = TypvalValue::Number(a | b);
}

/// `xor({expr}, {expr})` - bitwise XOR of two numbers (`f_xor`).
fn f_xor(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let a = crate::eval::typval::tv_get_number_chk(&argvars[0], None);
    let b = crate::eval::typval::tv_get_number_chk(&argvars[1], None);
    rettv.value = TypvalValue::Number(a ^ b);
}

/// `abs({expr})` - absolute value of a Number or Float (`f_abs`).
///
/// The original's own `Float` branch goes through `float_op_wrapper`/
/// `fabs` - not needed here (matching `TypvalValue::Float`'s own
/// payload directly and calling `f64::abs` is identical, and this
/// crate's `VimLFuncT` has no function-pointer-passing equivalent of
/// `EvalFuncData` yet anyway, see this module's own doc comment).
///
/// The non-`Float` branch uses `wrapping_neg` rather than a bare
/// negation/`i64::abs`: the original's own `n > 0 ? n : -n` has no
/// special case for `n == VARNUMBER_MIN` (unlike e.g. `num_divide`,
/// which explicitly special-cases the analogous `VARNUMBER_MIN / -1`
/// overflow) - `-VARNUMBER_MIN` is real, if obscure, signed-overflow
/// UB in the original C too. `wrapping_neg` reproduces the same
/// wrap-back-to-`VARNUMBER_MIN` result standard two's-complement
/// hardware actually produces for this, rather than introducing a NEW
/// panic (a bare `-n`/`n.abs()` would panic in a debug build) the
/// original never has.
fn f_abs(argvars: &[TypvalT], rettv: &mut TypvalT) {
    if let TypvalValue::Float(f) = argvars[0].value {
        rettv.value = TypvalValue::Float(f.abs());
        return;
    }
    let mut error = false;
    let n = crate::eval::typval::tv_get_number_chk(&argvars[0], Some(&mut error));
    rettv.value = TypvalValue::Number(if error {
        -1
    } else if n > 0 {
        n
    } else {
        n.wrapping_neg()
    });
}

/// Get the maximal/minimal Number value in a `List` or `Dict`
/// (`max_min`). Only assigns `rettv.value`'s own Number payload -
/// returns `0` for an empty `List`/`Dict`. A non-`List`/`Dict` `tv`
/// (the original's own `E712: Argument of max()/min() must be a List
/// or Dictionary` case) leaves `rettv` untouched, matching this
/// crate's established "skip the display, keep the caller's own
/// default" policy - as does a `List`/`Dict` item that isn't
/// Number-shaped (the original's own `errmsg already given` case).
///
/// # Safety
/// If `tv.value` is `List`/`Dict`-typed with a non-null pointer, that
/// pointer must be valid.
unsafe fn max_min(tv: &TypvalT, rettv: &mut TypvalT, domax: bool) {
    use crate::eval::typval_defs::{VARNUMBER_MAX, VARNUMBER_MIN};

    let mut n = if domax { VARNUMBER_MIN } else { VARNUMBER_MAX };
    match &tv.value {
        TypvalValue::List(l) => {
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { crate::eval::typval::tv_list_len(*l) } == 0 {
                return;
            }
            // SAFETY: forwarded from this function's own safety doc.
            let mut item = unsafe { crate::eval::typval::tv_list_first(*l) };
            while !item.is_null() {
                let mut error = false;
                // SAFETY: `item` is a live node reached by walking the
                // list just validated above.
                let i = crate::eval::typval::tv_get_number_chk(unsafe { &(*item).li_tv }, Some(&mut error));
                if error {
                    return;
                }
                if if domax { i > n } else { i < n } {
                    n = i;
                }
                // SAFETY: forwarded from this function's own safety doc.
                item = unsafe { (*item).li_next };
            }
        }
        TypvalValue::Dict(d) => {
            // SAFETY: forwarded from this function's own safety doc.
            if crate::eval::typval::tv_dict_len(unsafe { d.as_ref() }) == 0 {
                return;
            }
            // SAFETY: forwarded from this function's own safety doc.
            let items: Vec<*mut crate::eval::typval_defs::DictitemT> = unsafe { &**d }.dv_index.values().copied().collect();
            for item in items {
                let mut error = false;
                // SAFETY: `item` came from the dict's own live index above.
                let i = crate::eval::typval::tv_get_number_chk(unsafe { &(*item).di_tv }, Some(&mut error));
                if error {
                    return;
                }
                if if domax { i > n } else { i < n } {
                    n = i;
                }
            }
        }
        _ => return,
    }
    rettv.value = TypvalValue::Number(n);
}

/// `max({expr})` - maximal Number value in a `List` or `Dict` (`f_max`).
///
/// # Safety
/// Forwarded from [`max_min`]'s own safety doc.
unsafe fn f_max(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { max_min(&argvars[0], rettv, true) };
}

/// `min({expr})` - minimal Number value in a `List` or `Dict` (`f_min`).
///
/// # Safety
/// Forwarded from [`max_min`]'s own safety doc.
unsafe fn f_min(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { max_min(&argvars[0], rettv, false) };
}

/// `char2nr({expr} [, {utf8}])` - the character value of the first
/// character in `{expr}` (`f_char2nr`).
///
/// The optional second argument (`{utf8}`) is validated (must be
/// Number-shaped) but never actually consulted for behavior, matching
/// the original exactly: nvim always decodes UTF-8 internally
/// regardless of this argument's value, so it exists only for
/// external-compatibility argument-shape validation. Unlike the
/// original's own `argvars[1].v_type != VAR_UNKNOWN` sentinel check
/// (a fixed-size array slot left at its default `VAR_UNKNOWN` when a
/// trailing optional argument is omitted), `argvars` here is already
/// exactly as long as the number of arguments the caller actually
/// passed ([`call_internal_func`] already validated it's within
/// `min_argc..=max_argc`) - so "was the 2nd argument passed" is simply
/// `argvars.len() > 1`. Every other function here with a trailing
/// optional argument follows this same substitution.
///
/// A genuinely empty `{expr}` is handled explicitly
/// (`crate::mbyte::utf_ptr2char` panics on an empty slice, relying on
/// the original's own C-string NUL terminator as an implicit "first
/// byte" to decode instead - this crate's `tv_get_string` returns a
/// truly empty `Vec<u8>` for an empty Vimscript string with no such
/// terminator to fall back on; matches `path.rs`'s own
/// `utf_ptr2char_or_nul` precedent for the identical hazard).
fn f_char2nr(argvars: &[TypvalT], rettv: &mut TypvalT) {
    if argvars.len() > 1 && !crate::eval::typval::tv_check_num(&argvars[1]) {
        return;
    }
    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    let n = if s.is_empty() { 0 } else { crate::mbyte::utf_ptr2char(&s) };
    rettv.value = TypvalValue::Number(crate::eval::typval_defs::VarnumberT::from(n));
}

/// `nr2char({expr} [, {utf8}])` - a string holding the single
/// character whose codepoint is `{expr}` (`f_nr2char`). See
/// [`f_char2nr`]'s own doc comment for why the optional second
/// argument only validates its own shape without otherwise affecting
/// behavior, and why `argvars.len() > 1` replaces the original's
/// `argvars[1].v_type != VAR_UNKNOWN` sentinel check.
///
/// The original's own `E5070`/`E5071` out-of-range messages are
/// omitted (message display, not tractable) - `rettv` is simply left
/// untouched on either error, matching this crate's established
/// "skip the display, keep an otherwise-harmless default" policy.
fn f_nr2char(argvars: &[TypvalT], rettv: &mut TypvalT) {
    if argvars.len() > 1 && !crate::eval::typval::tv_check_num(&argvars[1]) {
        return;
    }
    let mut error = false;
    let num = crate::eval::typval::tv_get_number_chk(&argvars[0], Some(&mut error));
    if error || num < 0 || num > i64::from(i32::MAX) {
        return;
    }
    let mut buf = [0u8; crate::mbyte_defs::MB_MAXCHAR];
    let len = crate::mbyte::utf_char2bytes(num as i32, &mut buf);
    rettv.value = TypvalValue::String(Some(buf[..len as usize].to_vec()));
}

/// `str2float({expr})` - convert `{expr}` to a Float (`f_str2float`).
///
/// Hex-float syntax (e.g. `"0x1.8p3"`) inherits
/// [`crate::eval::eval::string2float`]'s own documented
/// `unimplemented!()` for that input shape. Unlike `eval_number`'s own
/// pre-validated caller (which can never actually reach that path),
/// an arbitrary user string passed to `str2float()` genuinely could
/// start with `"0x"` - a real, if narrow, reachability gap inherited
/// from `string2float` itself, not a new one introduced here.
fn f_str2float(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    let skip = crate::charset::skipwhite(&s);
    let mut p = &s[skip..];
    let isneg = p.first() == Some(&b'-');
    if matches!(p.first(), Some(&b'+') | Some(&b'-')) {
        let skip2 = crate::charset::skipwhite(&p[1..]);
        p = &p[1 + skip2..];
    }
    let (mut value, _consumed) = crate::eval::eval::string2float(p);
    if isneg {
        value *= -1.0;
    }
    rettv.value = TypvalValue::Float(value);
}

/// `str2nr({string} [, {base} [, {quoted}]])` - convert `{string}` to
/// a Number in a given base (`f_str2nr`).
///
/// Unusually, the original keeps this function (and `str2list()`) in
/// `strings.c` rather than `funcs.c` itself - noted here since every
/// other function in this module maps to `funcs.c`.
///
/// The original's own `E475: Invalid argument` message (for an
/// invalid `{base}`) is omitted, matching this crate's established
/// "skip the display, keep an otherwise-harmless default" policy -
/// `rettv` is simply left untouched. See [`f_char2nr`]'s own doc
/// comment for why `argvars.len() > N` replaces the original's
/// `argvars[N].v_type != VAR_UNKNOWN` sentinel checks for the two
/// trailing optional arguments here.
fn f_str2nr(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let mut base = 10;
    let mut what = 0;
    if argvars.len() > 1 {
        base = crate::eval::typval::tv_get_number(&argvars[1]);
        if base != 2 && base != 8 && base != 10 && base != 16 {
            return;
        }
        if argvars.len() > 2 && crate::eval::typval::tv_get_bool(&argvars[2]) != 0 {
            what |= crate::charset::STR2NR_QUOTE;
        }
    }

    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    let skip = crate::charset::skipwhite(&s);
    let mut p = &s[skip..];
    let isneg = p.first() == Some(&b'-');
    if matches!(p.first(), Some(&b'+') | Some(&b'-')) {
        let skip2 = crate::charset::skipwhite(&p[1..]);
        p = &p[1 + skip2..];
    }
    match base {
        2 => what |= crate::charset::STR2NR_BIN | crate::charset::STR2NR_FORCE,
        8 => what |= crate::charset::STR2NR_OCT | crate::charset::STR2NR_OOCT | crate::charset::STR2NR_FORCE,
        16 => what |= crate::charset::STR2NR_HEX | crate::charset::STR2NR_FORCE,
        _ => {}
    }
    let mut n = 0;
    crate::charset::vim_str2nr(p, None, None, what, Some(&mut n), None, 0, false, None);
    // Text after the number is silently ignored.
    rettv.value = TypvalValue::Number(if isneg { -n } else { n });
}

/// `str2list({string} [, {utf8}])` - convert `{string}` into a `List`
/// of character codepoints (`f_str2list`). Unusually lives in
/// `strings.c` in the original, like [`f_str2nr`] (see its own doc
/// comment).
///
/// A NUL byte inside `{string}` ends the conversion early, matching
/// the original's own `for (; *p != NUL; ...)` loop exactly - not a
/// shortcut: `tv_get_string` can return a `Vec<u8>` containing an
/// embedded NUL (unlike the original's own NUL-terminated `char *`,
/// which could never represent one to begin with), so this crate's
/// translation must check for it explicitly to reproduce the
/// identical stopping behavior (and to avoid an infinite loop:
/// `utf_ptr2len` itself returns `0` for a NUL byte, matching how the
/// original's loop condition would never even call it on one).
///
/// # Safety
/// `rettv`'s own value must not (yet) hold a `List`/`Dict`/`Blob`/
/// `Partial` pointer that anything else still depends on - this
/// function unconditionally overwrites it.
unsafe fn f_str2list(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    let l = unsafe {
        crate::eval::typval::tv_list_alloc_ret(rettv, crate::eval::typval_defs::ListLenSpecials::Unknown as isize)
    };
    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    let mut pos = 0;
    while pos < s.len() && s[pos] != 0 {
        let c = crate::mbyte::utf_ptr2char(&s[pos..]);
        // SAFETY: `l` was just allocated above by this same function.
        unsafe { crate::eval::typval::tv_list_append_number(l, i64::from(c)) };
        pos += crate::mbyte::utf_ptr2len(&s[pos..]) as usize;
    }
}

/// `list2str({list} [, {utf8}])` - convert a `List` of character
/// codepoints into a `String` (`f_list2str`). Unusually lives in
/// `eval/typval.c` in the original, neither `strings.c` nor
/// `funcs.c` (see [`f_str2nr`]'s own doc comment for the general
/// note).
///
/// The original's own `E475: Invalid argument` (a non-`List` argument)
/// is omitted, matching this crate's established "skip the display,
/// keep an otherwise-harmless default" policy - `rettv` is simply left
/// as an empty `String`, matching the original's own pre-set
/// `rettv->vval.v_string = NULL` default for this exact error path
/// too.
///
/// # Safety
/// If `argvars[0].value` is `List`-typed with a non-null pointer, that
/// pointer must be valid.
unsafe fn f_list2str(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::String(None);
    let TypvalValue::List(l) = &argvars[0].value else {
        return;
    };
    let l = *l;
    if l.is_null() {
        return; // empty list results in empty string
    }

    let mut result = Vec::new();
    let mut buf = [0u8; crate::mbyte_defs::MB_MAXBYTES];
    // SAFETY: forwarded from this function's own safety doc.
    let mut item = unsafe { crate::eval::typval::tv_list_first(l) };
    while !item.is_null() {
        // SAFETY: `item` is a live node reached by walking the list
        // just validated above.
        let n = crate::eval::typval::tv_get_number(unsafe { &(*item).li_tv });
        let buflen = crate::mbyte::utf_char2bytes(n as i32, &mut buf);
        result.extend_from_slice(&buf[..buflen as usize]);
        // SAFETY: forwarded from this function's own safety doc.
        item = unsafe { (*item).li_next };
    }
    rettv.value = TypvalValue::String(Some(result));
}

/// Apply a single-argument `f64` math function to `argvars[0]`,
/// storing the result as a `Float` (`float_op_wrapper`).
///
/// The original dispatches EVERY one of `sin()`/`cos()`/`sqrt()`/etc.
/// through this ONE shared C function, selected via a `func_float`
/// function pointer stored in each entry of the generated `functions[]`
/// table itself (`EvalFuncData.func_float`). This crate's own
/// `VimLFuncT` has no such extra per-entry payload - instead, each
/// `f_sin`/`f_cos`/etc. below is its own tiny function that already
/// "bakes in" which `f64` method to call, so `float_op_wrapper` only
/// needs `argvars`/`rettv` plus a plain function pointer parameter,
/// not a whole widened table-entry shape. Same data (a name maps to a
/// specific one-argument float transform), a different but
/// functionally-identical mechanism - matching this module's own
/// established `FUNCTIONS`-table-vs-perfect-hash precedent.
///
/// Returns `0.0` if `argvars[0]` isn't Number/Float-shaped, matching
/// the original's own fallback exactly (its own `E808` message is
/// omitted, see `tv_get_float_chk`'s own doc comment).
fn float_op_wrapper(argvars: &[TypvalT], rettv: &mut TypvalT, f: fn(f64) -> f64) {
    let result = crate::eval::typval::tv_get_float_chk(&argvars[0]).map_or(0.0, f);
    rettv.value = TypvalValue::Float(result);
}

/// Apply a two-argument `f64` math function to `argvars[0]`/
/// `argvars[1]`, storing the result as a `Float` - shared by
/// `atan2()`/`pow()`/`fmod()`, each of which hand-expands the
/// equivalent of [`float_op_wrapper`] inline in the original (there is
/// no separate, named 2-argument sibling of `float_op_wrapper` in the
/// original itself).
fn float_op2_wrapper(argvars: &[TypvalT], rettv: &mut TypvalT, f: fn(f64, f64) -> f64) {
    use crate::eval::typval::tv_get_float_chk;
    let result = match (tv_get_float_chk(&argvars[0]), tv_get_float_chk(&argvars[1])) {
        (Some(fx), Some(fy)) => f(fx, fy),
        _ => 0.0,
    };
    rettv.value = TypvalValue::Float(result);
}

/// `sin({expr})` (`f_sin`).
fn f_sin(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::sin);
}

/// `cos({expr})` (`f_cos`).
fn f_cos(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::cos);
}

/// `tan({expr})` (`f_tan`).
fn f_tan(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::tan);
}

/// `asin({expr})` (`f_asin`).
fn f_asin(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::asin);
}

/// `acos({expr})` (`f_acos`).
fn f_acos(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::acos);
}

/// `atan({expr})` (`f_atan`).
fn f_atan(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::atan);
}

/// `sinh({expr})` (`f_sinh`).
fn f_sinh(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::sinh);
}

/// `cosh({expr})` (`f_cosh`).
fn f_cosh(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::cosh);
}

/// `tanh({expr})` (`f_tanh`).
fn f_tanh(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::tanh);
}

/// `sqrt({expr})` (`f_sqrt`).
fn f_sqrt(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::sqrt);
}

/// `exp({expr})` (`f_exp`).
fn f_exp(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::exp);
}

/// `log({expr})` - natural logarithm (`f_log`).
fn f_log(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::ln);
}

/// `log10({expr})` - base-10 logarithm (`f_log10`).
fn f_log10(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::log10);
}

/// `floor({expr})` (`f_floor`).
fn f_floor(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::floor);
}

/// `ceil({expr})` (`f_ceil`).
fn f_ceil(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::ceil);
}

/// `round({expr})` - round half away from zero (`f_round`). Rust's own
/// `f64::round` uses the identical "round half away from zero" rule as
/// the original's own C `round()`, unlike `f64::round_ties_even`.
fn f_round(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::round);
}

/// `trunc({expr})` (`f_trunc`).
fn f_trunc(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::trunc);
}

/// `atan2({expr1}, {expr2})` (`f_atan2`).
fn f_atan2(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op2_wrapper(argvars, rettv, f64::atan2);
}

/// `pow({x}, {y})` (`f_pow`).
fn f_pow(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op2_wrapper(argvars, rettv, f64::powf);
}

/// `fmod({expr1}, {expr2})` (`f_fmod`). Rust's own `%` operator on
/// `f64` is specified to match C's `fmod()` exactly (truncating
/// division's remainder, sign matching the dividend) - NOT the
/// distinct, round-to-nearest-even IEEE 754 `remainder()` operation
/// (`f64::rem_euclid`'s own always-nonnegative result is a third,
/// again-different operation) - verified directly with a dedicated
/// test using a negative dividend, the one input shape where all
/// three would disagree.
fn f_fmod(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op2_wrapper(argvars, rettv, |a, b| a % b);
}

/// `float2nr({expr})` - convert a Float to a Number, clamping to the
/// representable `Number` range rather than following the original's
/// own `(varnumber_T)f` cast for a wildly out-of-range Float, which is
/// real (if effectively unreachable given the explicit clamp checks
/// immediately before it) signed-overflow UB in C (`f_float2nr`).
///
/// A `NaN` input reaches the final `f as VarnumberT` branch (both
/// clamp comparisons are false for `NaN`, matching the original's own
/// identical fallthrough) - Rust's own `as` cast saturates `NaN` to
/// `0` (well-defined, unlike the original's own UB for this exact
/// input), which this translation keeps rather than inventing new
/// behavior to paper over a case the original itself never defines.
fn f_float2nr(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let Some(f) = crate::eval::typval::tv_get_float_chk(&argvars[0]) else {
        return;
    };
    let max = crate::eval::typval_defs::VARNUMBER_MAX as f64;
    let n = if f <= -max + f64::EPSILON {
        -crate::eval::typval_defs::VARNUMBER_MAX
    } else if f >= max - f64::EPSILON {
        crate::eval::typval_defs::VARNUMBER_MAX
    } else {
        f as crate::eval::typval_defs::VarnumberT
    };
    rettv.value = TypvalValue::Number(n);
}

/// `tolower({string})` - convert to lowercase (`f_tolower`).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (forwarded from
/// `crate::strings::strcase_save`'s own safety doc).
unsafe fn f_tolower(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    // SAFETY: forwarded from this function's own safety doc.
    let mut result = unsafe { crate::strings::strcase_save(&s, false) };
    result.pop(); // strip strcase_save's own trailing NUL terminator.
    rettv.value = TypvalValue::String(Some(result));
}

/// `toupper({string})` - convert to uppercase (`f_toupper`).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (forwarded from
/// `crate::strings::strcase_save`'s own safety doc).
unsafe fn f_toupper(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    // SAFETY: forwarded from this function's own safety doc.
    let mut result = unsafe { crate::strings::strcase_save(&s, true) };
    result.pop();
    rettv.value = TypvalValue::String(Some(result));
}

/// `trim({text} [, {mask} [, {dir}]])` - trim characters from the
/// start and/or end of `{text}` (`f_trim`).
///
/// `{mask}` defaults to trimming ASCII whitespace plus the
/// non-breaking space U+00A0 (`c > ' ' && c != 0xa0` in the original -
/// "not trim-worthy" exactly when a character is neither ASCII
/// whitespace nor NBSP, matching Vim's own documented default).
/// `{dir}`: `0` (default) trims both ends, `1` leading only, `2`
/// trailing only - like the original, only ever read when `{mask}`
/// was ACTUALLY passed as a real `String` (an omitted `{mask}` means
/// `{dir}` is never consulted even if somehow present, exactly
/// mirroring the original's own nested
/// `if (argvars[1].v_type == VAR_STRING) { ... if (argvars[2]...) }`
/// structure).
///
/// A NUL byte inside `{text}`/`{mask}` ends each string early,
/// matching the original's own NUL-terminated `strlen`/`*head != NUL`
/// loop conditions exactly - the same established "embedded NUL ends
/// a C-string-modeled scan" translation `f_str2list`'s own doc comment
/// explains in more detail.
///
/// The original's own `E475: Invalid argument` (`{dir}` out of the
/// `0..=2` range) is omitted, matching this crate's established "skip
/// the display, keep an otherwise-harmless default" policy - `rettv`
/// is simply left as an empty `String`, matching the original's own
/// pre-set `rettv->vval.v_string = NULL` default for this exact error
/// path too.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (forwarded from
/// `crate::mbyte::utf_head_off`'s own safety doc).
unsafe fn f_trim(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::String(None);

    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());

    if argvars.len() > 1 && !matches!(argvars[1].value, TypvalValue::String(_)) {
        return;
    }

    let mut mask: Option<Vec<u8>> = None;
    let mut dir: crate::eval::typval_defs::VarnumberT = 0;
    if argvars.len() > 1 {
        let m = crate::eval::typval::tv_get_string(&argvars[1]);
        mask = if m.first() == Some(&0) || m.is_empty() { None } else { Some(m) };

        if argvars.len() > 2 {
            let mut error = false;
            dir = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error));
            if error || !(0..=2).contains(&dir) {
                return;
            }
        }
    }

    let is_trim_worthy = |c: i32, mask: &Option<Vec<u8>>| -> bool {
        match mask {
            None => !(c > i32::from(b' ') && c != 0xa0),
            Some(m) => {
                let mend = m.iter().position(|&b| b == 0).unwrap_or(m.len());
                let mut p = 0;
                while p < mend {
                    if crate::mbyte::utf_ptr2char(&m[p..mend]) == c {
                        return true;
                    }
                    p += crate::mbyte::utf_ptr2len(&m[p..mend]) as usize;
                }
                false
            }
        }
    };

    let mut head = 0usize;
    if dir == 0 || dir == 1 {
        while head < end {
            let c1 = crate::mbyte::utf_ptr2char(&s[head..end]);
            if !is_trim_worthy(c1, &mask) {
                break;
            }
            head += crate::mbyte::utf_ptr2len(&s[head..end]) as usize;
        }
    }

    // utf_head_off requires its own `base` to include a trailing NUL
    // byte - build one once, up front, for the trailing-trim walk
    // below (the only place this function needs it).
    let mut nul_terminated = s[..end].to_vec();
    nul_terminated.push(0);

    let mut tail = end;
    if dir == 0 || dir == 2 {
        while tail > head {
            let mut prev = tail - 1;
            // SAFETY: forwarded from this function's own safety doc.
            let head_off = unsafe { crate::mbyte::utf_head_off(&nul_terminated, prev) };
            prev -= head_off as usize;
            let c1 = crate::mbyte::utf_ptr2char(&s[prev..end]);
            if !is_trim_worthy(c1, &mask) {
                break;
            }
            tail = prev;
        }
    }

    rettv.value = TypvalValue::String(Some(s[head..tail].to_vec()));
}

/// `has_key({dict}, {key})` - whether `{dict}` has a key `{key}`
/// (`f_has_key`).
///
/// The original's own `E715: Dictionary required` (a non-`Dict`
/// argument) is omitted, matching this crate's established "skip the
/// display, keep an otherwise-harmless default" policy - `rettv` is
/// simply left untouched (still its caller's own default-initialized
/// `Number(0)`).
fn f_has_key(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let TypvalValue::Dict(d) = &argvars[0].value else {
        return;
    };
    if d.is_null() {
        return;
    }
    let key = crate::eval::typval::tv_get_string(&argvars[1]);
    // SAFETY: `d` is non-null here, and every `Dict`-typed `argvars[0]`
    // reaching this point must already carry a valid pointer, matching
    // every other function in this crate touching that type.
    let found = crate::eval::typval::tv_dict_has_key(unsafe { d.as_mut() }, &key);
    rettv.value = TypvalValue::Number(i64::from(found));
}

/// What to place in each resulting list item for [`tv_dict2list`]
/// (`DictListType`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum DictListType {
    Keys,
    Values,
    Items,
}

/// Turn a dictionary into a `List` (`tv_dict2list`).
///
/// Scoped to `argvars[0]` always being `Dict`-typed (or absent
/// entirely, i.e. a type error) - matches `keys()`/`values()`'s own
/// real shape exactly; `items()` additionally accepts a `String`/
/// `List`/`Blob` in the original (via `tv_string2items`/
/// `tv_list2items`/`tv_blob2items`), none of which are translated yet,
/// so [`f_items`] itself declines those cases explicitly rather than
/// silently mishandling them here.
///
/// The original's own `E715: Dictionary required` (a non-`Dict`
/// argument) is omitted, matching this crate's established "skip the
/// display, keep an otherwise-harmless default" policy - an empty
/// `List` is returned instead, matching the original's own
/// `tv_list_alloc_ret(rettv, 0); return;` for this exact error path.
///
/// # Safety
/// If `argvars[0].value` is `Dict`-typed with a non-null pointer, that
/// pointer must be valid.
unsafe fn tv_dict2list(argvars: &[TypvalT], rettv: &mut TypvalT, what: DictListType) {
    let TypvalValue::Dict(d) = &argvars[0].value else {
        // SAFETY: forwarded from this function's own safety doc.
        let _ = unsafe { crate::eval::typval::tv_list_alloc_ret(rettv, 0) };
        return;
    };
    let d = *d;
    let len = crate::eval::typval::tv_dict_len(unsafe { d.as_ref() });
    // SAFETY: forwarded from this function's own safety doc.
    let l = unsafe { crate::eval::typval::tv_list_alloc_ret(rettv, len as isize) };
    if d.is_null() {
        return; // NULL dict behaves like an empty dict.
    }

    // SAFETY: forwarded from this function's own safety doc.
    let items: Vec<*mut crate::eval::typval_defs::DictitemT> = unsafe { &*d }.dv_index.values().copied().collect();
    for item in items {
        // SAFETY: `item` came from the dict's own live index above.
        let di = unsafe { &*item };
        // di_key always carries a trailing NUL terminator (matching
        // hi_key's C-string contract - see tv_dict_item_alloc's own
        // doc comment), which a Vimscript String value must NOT
        // include - strip it here, matching the same established
        // idiom used elsewhere in this crate (e.g. typval.rs's own
        // tv_dict_equal).
        let key = &di.di_key[..di.di_key.len() - 1];
        match what {
            DictListType::Keys => {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::eval::typval::tv_list_append_string(l, Some(key)) };
            }
            DictListType::Values => {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::eval::typval::tv_list_append_tv(l, &di.di_tv) };
            }
            DictListType::Items => {
                let sub_l = crate::eval::typval::tv_list_alloc(2);
                // SAFETY: `sub_l` was just allocated above, a fresh
                // pointer not shared with anything yet.
                unsafe { crate::eval::typval::tv_list_ref(sub_l) };
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::eval::typval::tv_list_append_string(sub_l, Some(key)) };
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::eval::typval::tv_list_append_tv(sub_l, &di.di_tv) };
                let tv_item = TypvalT { value: TypvalValue::List(sub_l), ..Default::default() };
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::eval::typval::tv_list_append_owned_tv(l, tv_item) };
            }
        }
    }
}

/// `keys({dict})` - a `List` of `{dict}`'s own keys (`f_keys`).
///
/// # Safety
/// Forwarded from [`tv_dict2list`]'s own safety doc.
unsafe fn f_keys(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_dict2list(argvars, rettv, DictListType::Keys) };
}

/// `values({dict})` - a `List` of `{dict}`'s own values (`f_values`).
///
/// # Safety
/// Forwarded from [`tv_dict2list`]'s own safety doc.
unsafe fn f_values(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_dict2list(argvars, rettv, DictListType::Values) };
}

/// `items({dict})` - a `List` of `[key, value]` pairs from `{dict}`
/// (`f_items`).
///
/// Only the `Dict` case is translated - the original also accepts a
/// `String` (character-by-character), `List` (index/value pairs), or
/// `Blob` (byte-index/value pairs) via `tv_string2items`/
/// `tv_list2items`/`tv_blob2items`, none of which exist yet. Panics
/// via `unimplemented!()` for those, rather than silently returning an
/// empty/wrong result.
///
/// # Safety
/// Forwarded from [`tv_dict2list`]'s own safety doc.
unsafe fn f_items(argvars: &[TypvalT], rettv: &mut TypvalT) {
    match &argvars[0].value {
        TypvalValue::String(_) => {
            unimplemented!("f_items: a String argument needs tv_string2items, not yet translated")
        }
        TypvalValue::List(_) => {
            unimplemented!("f_items: a List argument needs tv_list2items, not yet translated")
        }
        TypvalValue::Blob(_) => {
            unimplemented!("f_items: a Blob argument needs tv_blob2items, not yet translated")
        }
        // SAFETY: forwarded from this function's own safety doc.
        _ => unsafe { tv_dict2list(argvars, rettv, DictListType::Items) },
    }
}

/// `get({list}, {idx} [, {default}])` / `get({blob}, {idx} [,
/// {default}])` / `get({dict}, {key} [, {default}])` - a generic
/// getter with an optional default for a missing/out-of-range entry
/// (`f_get`).
///
/// Only the `Blob`/`List`/`Dict` cases are translated - the original
/// also accepts a `Funcref`/`Partial` (returning its own `"func"`/
/// `"name"`/`"dict"`/`"args"`/`"arity"` introspection sub-fields, via
/// `get_func_arity`, not yet translated); panics via `unimplemented!()`
/// for that case rather than silently mishandling it. Any other type
/// (matching the original's own `semsg(_(e_listdictblobarg), ...)`
/// case) falls through to the shared "not found" tail below, exactly
/// like the original's own `tv` staying `NULL`.
///
/// The original's own Blob-success path sets `tv = rettv` (a
/// self-alias, then a same-value `tv_copy(tv, rettv)` at the very
/// end; `tv_copy`'s own doc comment explicitly permits `from`/`to`
/// pointing at the same location), then this translation instead
/// returns directly after setting `rettv`, an observably identical
/// outcome (a plain `Number` has no shared/refcounted state a self-
/// copy could otherwise matter for) without needing to alias a
/// `*const` pointer against the same `&mut TypvalT` still used later.
///
/// # Safety
/// If `argvars[0].value` is `Blob`/`List`/`Dict`-typed with a non-null
/// pointer, that pointer must be valid. Forwards [`tv_copy`]'s own
/// safety doc for `argvars[2]` (the default, if given).
unsafe fn f_get(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let mut found: Option<*const TypvalT> = None;

    match &argvars[0].value {
        TypvalValue::Blob(b) => {
            let b = *b;
            let mut error = false;
            let idx = crate::eval::typval::tv_get_number_chk(&argvars[1], Some(&mut error)) as i32;
            if !error {
                // SAFETY: forwarded from this function's own safety doc.
                let len = unsafe { crate::eval::typval::tv_blob_len(b) };
                let idx = if idx < 0 { len + idx } else { idx };
                if idx < 0 || idx >= len {
                    rettv.value = TypvalValue::Number(-1);
                } else {
                    // SAFETY: forwarded from this function's own safety doc.
                    rettv.value =
                        TypvalValue::Number(i64::from(unsafe { crate::eval::typval::tv_blob_get(b, idx) }));
                    return;
                }
            }
        }
        TypvalValue::List(l) => {
            let l = *l;
            if !l.is_null() {
                let mut error = false;
                let idx = crate::eval::typval::tv_get_number_chk(&argvars[1], Some(&mut error)) as i32;
                if !error {
                    // SAFETY: forwarded from this function's own safety doc.
                    let li = unsafe { crate::eval::typval::tv_list_find(l, idx) };
                    if !li.is_null() {
                        // SAFETY: `li` is a live node just returned by
                        // `tv_list_find` above.
                        found = Some(unsafe { std::ptr::addr_of!((*li).li_tv) });
                    }
                }
            }
        }
        TypvalValue::Dict(d) => {
            let d = *d;
            if !d.is_null() {
                let key = crate::eval::typval::tv_get_string(&argvars[1]);
                // SAFETY: forwarded from this function's own safety doc.
                if let Some(di) = crate::eval::typval::tv_dict_find(unsafe { d.as_mut() }, &key) {
                    // SAFETY: `di` is a live item just returned by
                    // `tv_dict_find` above.
                    found = Some(unsafe { std::ptr::addr_of!((*di).di_tv) });
                }
            }
        }
        TypvalValue::Func(_) | TypvalValue::Partial(_) => {
            unimplemented!(
                "f_get: a Funcref/Partial argument needs get_func_arity and partial \
                 introspection, not yet translated"
            );
        }
        _ => {}
    }

    match found {
        None => {
            if argvars.len() > 2 {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::eval::typval::tv_copy(&argvars[2], rettv) };
            }
        }
        // SAFETY: forwarded from this function's own safety doc; `tv`
        // was just derived above from a live List/Dict item.
        Some(tv) => unsafe { crate::eval::typval::tv_copy(&*tv, rettv) },
    }
}

/// `index({object}, {expr} [, {start} [, {ic}]])` - the index of the
/// first item in `{object}` equal to `{expr}`, or `-1` if not found
/// (`f_index`).
///
/// Any type other than `Blob`/`List` (matching the original's own
/// `emsg(_(e_listblobreq))` case) leaves `rettv` at `-1`, exactly like
/// the original's own pre-set default.
///
/// # Safety
/// If `argvars[0].value` is `Blob`/`List`-typed with a non-null
/// pointer, that pointer must be valid. Forwards
/// [`crate::eval::typval::tv_equal`]'s own safety doc for every item
/// compared against `argvars[1]`.
unsafe fn f_index(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(-1);

    if let TypvalValue::Blob(b) = &argvars[0].value {
        let b = *b;
        let mut start = 0;
        if argvars.len() > 2 {
            let mut error = false;
            start = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error)) as i32;
            if error {
                return;
            }
        }
        if b.is_null() {
            return;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let len = unsafe { crate::eval::typval::tv_blob_len(b) };
        if start < 0 {
            start = (len + start).max(0);
        }
        for idx in start..len {
            let byte_tv = TypvalT {
                // SAFETY: forwarded from this function's own safety doc.
                value: TypvalValue::Number(i64::from(unsafe { crate::eval::typval::tv_blob_get(b, idx) })),
                ..Default::default()
            };
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { crate::eval::typval::tv_equal(&byte_tv, &argvars[1], false) } {
                rettv.value = TypvalValue::Number(i64::from(idx));
                return;
            }
        }
        return;
    }

    let TypvalValue::List(l) = &argvars[0].value else {
        return;
    };
    let l = *l;
    if l.is_null() {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let mut item = unsafe { crate::eval::typval::tv_list_first(l) };
    let mut idx = 0;
    let mut ic = false;
    if argvars.len() > 2 {
        let mut error = false;
        let start_n = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error)) as i32;
        // SAFETY: forwarded from this function's own safety doc.
        let uidx = unsafe { crate::eval::typval::tv_list_uidx(l, start_n) };
        if error || uidx == -1 {
            item = std::ptr::null_mut();
        } else {
            idx = uidx;
            // SAFETY: forwarded from this function's own safety doc.
            item = unsafe { crate::eval::typval::tv_list_find(l, idx) };
        }
        if argvars.len() > 3 {
            ic = crate::eval::typval::tv_get_number_chk(&argvars[3], Some(&mut error)) != 0;
            if error {
                item = std::ptr::null_mut();
            }
        }
    }

    while !item.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::eval::typval::tv_equal(&(*item).li_tv, &argvars[1], ic) } {
            rettv.value = TypvalValue::Number(i64::from(idx));
            return;
        }
        // SAFETY: forwarded from this function's own safety doc.
        item = unsafe { (*item).li_next };
        idx += 1;
    }
}

/// Reverse text into a newly-allocated `Vec<u8>` (`reverse_text`).
///
/// A NUL byte inside `s` ends the reversal early, matching the
/// original's own NUL-terminated `strlen(s)` bound exactly - the same
/// established "embedded NUL ends a C-string-modeled scan" idiom used
/// elsewhere in this module (e.g. `f_str2list`'s own doc comment).
///
/// # Safety
/// Forwarded from `crate::mbyte::utfc_ptr2len`'s own safety doc.
unsafe fn reverse_text(s: &[u8]) -> Vec<u8> {
    let len = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    let mut rev = vec![0u8; len];
    let mut s_i = 0;
    let mut rev_i = len;
    while s_i < len {
        // SAFETY: forwarded from this function's own safety doc.
        let mb_len = unsafe { crate::mbyte::utfc_ptr2len(&s[s_i..len]) } as usize;
        rev_i -= mb_len;
        rev[rev_i..rev_i + mb_len].copy_from_slice(&s[s_i..s_i + mb_len]);
        s_i += mb_len;
    }
    rev
}

/// `reverse({object})` - reverse a `Blob`/`List`/`String` in place
/// (`f_reverse`).
///
/// The original's own `E1252`-style "not a List/Blob/String" argument-
/// type message is omitted, matching this crate's established "skip
/// the display, keep an otherwise-harmless default" policy - `rettv`
/// is simply left untouched for any other type. A locked `List`
/// (matching the original's own `value_check_lock` guard) is likewise
/// left untouched.
///
/// # Safety
/// If `argvars[0].value` is `Blob`/`List`-typed with a non-null
/// pointer, that pointer must be valid.
unsafe fn f_reverse(argvars: &[TypvalT], rettv: &mut TypvalT) {
    match &argvars[0].value {
        TypvalValue::Blob(b) => {
            let b = *b;
            // SAFETY: forwarded from this function's own safety doc.
            let len = unsafe { crate::eval::typval::tv_blob_len(b) };
            for i in 0..len / 2 {
                // SAFETY: forwarded from this function's own safety doc.
                let tmp = unsafe { crate::eval::typval::tv_blob_get(b, i) };
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    let other = crate::eval::typval::tv_blob_get(b, len - i - 1);
                    crate::eval::typval::tv_blob_set(b, i, other);
                    crate::eval::typval::tv_blob_set(b, len - i - 1, tmp);
                }
            }
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_blob_set_ret(rettv, b) };
        }
        TypvalValue::String(s) => {
            // SAFETY: forwarded from this function's own safety doc.
            rettv.value = TypvalValue::String(s.as_ref().map(|s| unsafe { reverse_text(s) }));
        }
        TypvalValue::List(l) => {
            let l = *l;
            // SAFETY: forwarded from this function's own safety doc.
            let locked = unsafe { crate::eval::typval::tv_list_locked(l) };
            if !crate::eval::typval::value_check_lock(locked, None) {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    crate::eval::typval::tv_list_reverse(l);
                    crate::eval::typval::tv_list_set_ret(rettv, l);
                }
            }
        }
        _ => {}
    }
}

/// Count the number of times `needle` occurs in `haystack`
/// (`count_string`).
///
/// A `None` `haystack`/`needle`, or an empty `needle`, always counts
/// as `0` - matches the original's own `p == NULL || needle == NULL
/// || *needle == NUL` guard exactly. A NUL byte inside `haystack`
/// ends the count early, matching the original's own NUL-terminated
/// `strstr`/`while (*p != NUL)` scans - the same established
/// "embedded NUL ends a C-string-modeled scan" idiom used elsewhere in
/// this module.
fn count_string(haystack: Option<&[u8]>, needle: Option<&[u8]>, ic: bool) -> crate::eval::typval_defs::VarnumberT {
    let (Some(haystack), Some(needle)) = (haystack, needle) else {
        return 0;
    };
    if needle.is_empty() {
        return 0;
    }
    let len = haystack.iter().position(|&b| b == 0).unwrap_or(haystack.len());
    let haystack = &haystack[..len];

    let mut n = 0;
    let mut pos = 0;
    if ic {
        while pos < haystack.len() {
            if pos + needle.len() <= haystack.len()
                && crate::mbyte::mb_strnicmp(&haystack[pos..], needle, needle.len()) == 0
            {
                n += 1;
                pos += needle.len();
            } else {
                pos += crate::mbyte::utf_ptr2len(&haystack[pos..]).max(1) as usize;
            }
        }
    } else {
        while let Some(rel) = haystack[pos..].windows(needle.len()).position(|w| w == needle) {
            n += 1;
            pos += rel + needle.len();
        }
    }
    n
}

/// Count the number of times `needle` occurs in `l`, starting at
/// index `idx` (`count_list`).
///
/// # Safety
/// `l`, if non-null, must be a valid pointer to a live `ListT`.
/// Forwards [`crate::eval::typval::tv_equal`]'s own safety doc for
/// every item compared against `needle`.
unsafe fn count_list(
    l: *mut crate::eval::typval_defs::ListT,
    needle: &TypvalT,
    idx: i32,
    ic: bool,
) -> crate::eval::typval_defs::VarnumberT {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::eval::typval::tv_list_len(l) } == 0 {
        return 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let mut item = unsafe { crate::eval::typval::tv_list_find(l, idx) };
    if item.is_null() {
        // Matches the original's own `E984: List index out of range`
        // (message display omitted).
        return 0;
    }

    let mut n = 0;
    while !item.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::eval::typval::tv_equal(&(*item).li_tv, needle, ic) } {
            n += 1;
        }
        // SAFETY: forwarded from this function's own safety doc.
        item = unsafe { (*item).li_next };
    }
    n
}

/// Count the number of times `needle` occurs among `d`'s own values
/// (`count_dict`).
///
/// # Safety
/// `d`, if non-null, must be a valid pointer to a live `DictT`.
/// Forwards [`crate::eval::typval::tv_equal`]'s own safety doc for
/// every value compared against `needle`.
unsafe fn count_dict(
    d: *mut crate::eval::typval_defs::DictT,
    needle: &TypvalT,
    ic: bool,
) -> crate::eval::typval_defs::VarnumberT {
    if d.is_null() {
        return 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let items: Vec<*mut crate::eval::typval_defs::DictitemT> = unsafe { &*d }.dv_index.values().copied().collect();
    let mut n = 0;
    for item in items {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::eval::typval::tv_equal(&(*item).di_tv, needle, ic) } {
            n += 1;
        }
    }
    n
}

/// `count({comp}, {expr} [, {ic} [, {start}]])` - count the number of
/// times `{expr}` occurs in `{comp}` (a `String`/`List`/`Dict`)
/// (`f_count`).
///
/// `{start}` (the 4th argument) is only ever consulted for a `List`
/// when `{ic}` (the 3rd argument) was ALSO passed, matching the
/// original's own nested `argvars[2].v_type != VAR_UNKNOWN &&
/// argvars[3].v_type != VAR_UNKNOWN` check exactly (the same "a
/// trailing optional argument is only read when an EARLIER one was
/// also given" shape [`f_trim`]'s own doc comment already explains).
/// For a `Dict`, giving BOTH `{ic}` and `{start}` is the original's
/// own `E118`-style error case (a start index makes no sense for a
/// `Dict`) - message display omitted, `rettv` simply stays `0`.
///
/// Any type other than `String`/`List`/`Dict` (matching the original's
/// own `semsg(...)` case) also leaves `rettv` at `0`.
///
/// # Safety
/// If `argvars[0].value` is `List`/`Dict`-typed with a non-null
/// pointer, that pointer must be valid.
unsafe fn f_count(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let mut error = false;
    let mut ic = 0;
    if argvars.len() > 2 {
        ic = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error)) as i32;
    }

    let mut n: crate::eval::typval_defs::VarnumberT = 0;
    if !error {
        match &argvars[0].value {
            TypvalValue::String(haystack) => {
                let needle = crate::eval::typval::tv_get_string_chk(&argvars[1]);
                n = count_string(haystack.as_deref(), needle.as_deref(), ic != 0);
            }
            TypvalValue::List(l) => {
                let l = *l;
                let mut idx = 0;
                if argvars.len() > 3 {
                    idx = crate::eval::typval::tv_get_number_chk(&argvars[3], Some(&mut error));
                }
                if !error {
                    // SAFETY: forwarded from this function's own safety doc.
                    n = unsafe { count_list(l, &argvars[1], idx as i32, ic != 0) };
                }
            }
            TypvalValue::Dict(d) => {
                let d = *d;
                if !d.is_null() && argvars.len() <= 3 {
                    // SAFETY: forwarded from this function's own safety doc.
                    n = unsafe { count_dict(d, &argvars[1], ic != 0) };
                }
            }
            _ => {}
        }
    }
    rettv.value = TypvalValue::Number(n);
}

/// `copy({expr})` - make a shallow copy of `{expr}` (`f_copy`).
///
/// Mirrors the original's own `var_item_copy(NULL, &argvars[0], rettv,
/// false, 0)` call exactly - `conv=NULL`/`deep=false`/`copyID=0` are
/// the ONLY values `copy()` itself ever passes (only `deepcopy()`,
/// not yet translated, ever passes `deep=true`/a real `copyID`), so
/// this directly inlines `var_item_copy`'s own switch for exactly this
/// fixed parameter set, rather than translating the full, more
/// general `var_item_copy` (whose `copyID != 0` "use-the-copy-made-
/// earlier" branches are unreachable dead code for this specific,
/// always-`copyID`-`0` caller).
///
/// `List`/`Dict` assign [`crate::eval::typval::tv_list_copy`]/
/// [`crate::eval::typval::tv_dict_copy`]'s own result DIRECTLY (not
/// via `tv_list_set_ret`/`tv_dict_set_ret`, which would ref-count a
/// SECOND time) - both copy functions already set the new container's
/// own refcount to `1` internally themselves, exactly matching the
/// original's own direct `to->vval.v_list = tv_list_copy(...)`/
/// `to->vval.v_dict = tv_dict_copy(...)` field assignments (no
/// separate `tv_list_ref`/`dv_refcount++` at this call site either).
///
/// # Safety
/// If `argvars[0].value` is `List`/`Dict`/`Blob`/`Partial`-typed with
/// a non-null pointer, that pointer must be valid.
unsafe fn f_copy(argvars: &[TypvalT], rettv: &mut TypvalT) {
    match &argvars[0].value {
        TypvalValue::List(l) => {
            // SAFETY: forwarded from this function's own safety doc.
            let copy = unsafe { crate::eval::typval::tv_list_copy(std::ptr::null(), *l, false, 0) };
            rettv.value = TypvalValue::List(copy);
            rettv.v_lock = crate::eval::typval_defs::VarLockStatus::Unlocked;
        }
        TypvalValue::Blob(b) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_blob_copy(*b, rettv) };
        }
        TypvalValue::Dict(d) => {
            // SAFETY: forwarded from this function's own safety doc.
            let copy = unsafe { crate::eval::typval::tv_dict_copy(std::ptr::null(), *d, false, 0) };
            rettv.value = TypvalValue::Dict(copy);
            rettv.v_lock = crate::eval::typval_defs::VarLockStatus::Unlocked;
        }
        TypvalValue::Unknown => {
            debug_assert!(false, "f_copy(UNKNOWN) - internal_error in the original");
        }
        _ => {
            // Number/Float/Func/Partial/Bool/Special/String: always a
            // plain tv_copy in the original too (String's own
            // "conv == NULL" branch is always taken here).
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_copy(&argvars[0], rettv) };
        }
    }
}

/// `deepcopy({expr} [, {noref}])` - make a deep copy of `{expr}`
/// (`f_deepcopy`).
///
/// Unlike [`f_copy`], nested `List`/`Dict` items are ALSO copied,
/// recursively, via [`crate::eval::eval::var_item_copy`].
///
/// `{noref}` (default `0`/omitted) controls cycle handling: when
/// `0`, a container referenced more than once (e.g. `let l = [1]` /
/// `let outer = [l, l]`) is only copied ONCE - every reference in the
/// result points to that SAME single copy, matching the original
/// list's own sharing exactly (and correctly handling genuine cycles,
/// e.g. a list containing itself). When `{noref}` is `1`, every
/// occurrence gets its OWN separate copy instead - which means a
/// genuine cycle would recurse forever, hence [`DICT_MAXNEST`]'s
/// recursion limit turning that runaway recursion into a clean `FAIL`
/// instead of a stack overflow (matching the original's own
/// documented "a cyclic reference causes deepcopy() to fail" note for
/// `{noref}=1`).
///
/// # Safety
/// Forwarded from [`crate::eval::eval::var_item_copy`]'s own safety
/// doc, applied recursively through every nested List/Dict item
/// reachable from `argvars[0]`.
unsafe fn f_deepcopy(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // argvars.len() > 1 replaces the original's own argvars[1].v_type
    // != VAR_UNKNOWN sentinel check (this crate's argvars is already
    // exactly as long as what was actually passed, unlike the
    // original's own fixed-size, sentinel-padded array) -
    // tv_check_for_opt_bool_arg itself indexes argvars[1] directly,
    // so it must only ever be called when that index genuinely
    // exists; skipping it entirely when the optional arg is absent
    // produces the identical net effect, since tv_check_for_opt_bool_arg's
    // own first check is precisely "argvars[1] unknown -> OK" anyway.
    if argvars.len() > 1 && crate::eval::typval::tv_check_for_opt_bool_arg(argvars, 1) == crate::vim_defs::FAIL {
        return;
    }

    let mut noref: crate::eval::typval_defs::VarnumberT = 0;
    if argvars.len() > 1 {
        noref = crate::eval::typval::tv_get_bool_chk(&argvars[1], None);
    }

    let copy_id = if noref == 0 { crate::eval::eval::get_copy_id() } else { 0 };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::eval::var_item_copy(std::ptr::null(), &argvars[0], rettv, true, copy_id) };
}

/// `add({object}, {expr})` - append `{expr}` to `{object}` (a `List`
/// or `Blob`) (`f_add`, `eval/list.c`).
///
/// Returns the resulting `List`/`Blob` - the SAME container, mutated
/// in place (not a copy) - or `1` ("failed") if `{object}` is neither
/// (the original's own `emsg(_(e_listblobreq))` is omitted - message
/// display, not tractable; the identical default `rettv` value is
/// kept).
///
/// # Safety
/// If `argvars[0].value` is `List`/`Blob`-typed with a non-null
/// pointer, that pointer must be valid.
unsafe fn f_add(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(1); // Default: failed.
    match &argvars[0].value {
        TypvalValue::List(l) => {
            let l = *l;
            // SAFETY: forwarded from this function's own safety doc.
            let locked = unsafe { crate::eval::typval::tv_list_locked(l) };
            if !crate::eval::typval::value_check_lock(locked, None) {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    crate::eval::typval::tv_list_append_tv(l, &argvars[1]);
                    crate::eval::typval::tv_copy(&argvars[0], rettv);
                }
            }
        }
        TypvalValue::Blob(b) => {
            let b = *b;
            if !b.is_null() {
                // SAFETY: forwarded from this function's own safety doc.
                let locked = unsafe { (*b).bv_lock };
                if !crate::eval::typval::value_check_lock(locked, None) {
                    let mut error = false;
                    let n = crate::eval::typval::tv_get_number_chk(&argvars[1], Some(&mut error));
                    if !error {
                        // SAFETY: forwarded from this function's own safety doc.
                        unsafe {
                            (*b).bv_ga.ga_append(n as u8);
                            crate::eval::typval::tv_copy(&argvars[0], rettv);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// `insert({object}, {item} [, {idx}])` - insert `{item}` into
/// `{object}` (a `List` or `Blob`) before index `{idx}` (default `0`,
/// i.e. at the start) (`f_insert`, `eval/list.c`).
///
/// Returns the resulting `List`/`Blob` (the SAME container, mutated
/// in place) via [`crate::eval::typval::tv_copy`], or leaves `rettv`
/// at its caller-provided default (`Number(0)`) on any failure -
/// unlike [`f_add`], this function never sets its own explicit
/// "failed" sentinel, exactly matching the original's own structure
/// (every error path is a bare early `return`).
///
/// # Safety
/// If `argvars[0].value` is `List`/`Blob`-typed with a non-null
/// pointer, that pointer must be valid.
unsafe fn f_insert(argvars: &[TypvalT], rettv: &mut TypvalT) {
    match &argvars[0].value {
        TypvalValue::Blob(b) => {
            let b = *b;
            if b.is_null() {
                return;
            }
            // SAFETY: forwarded from this function's own safety doc.
            let locked = unsafe { (*b).bv_lock };
            if crate::eval::typval::value_check_lock(locked, None) {
                return;
            }

            // SAFETY: forwarded from this function's own safety doc.
            let len = unsafe { crate::eval::typval::tv_blob_len(b) };
            let mut before: i32 = 0;
            // argvars.len() > 2 replaces the original's own
            // argvars[2].v_type != VAR_UNKNOWN sentinel check.
            if argvars.len() > 2 {
                let mut error = false;
                before = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error)) as i32;
                if error {
                    return; // type error; errmsg already given in the original.
                }
                if before < 0 || before > len {
                    return; // semsg(_(e_invarg2), ...) omitted - see this module's own doc comment.
                }
            }
            let mut error = false;
            let val = crate::eval::typval::tv_get_number_chk(&argvars[1], Some(&mut error));
            if error {
                return;
            }
            if !(0..=255).contains(&val) {
                return; // semsg(_(e_invarg2), ...) omitted.
            }

            // SAFETY: forwarded from this function's own safety doc.
            unsafe { (*b).bv_ga.ga_grow(1) };
            // SAFETY: ga_grow just ensured ga_data has at least
            // len+1 bytes of real capacity - shifting [before, len)
            // right by one within that capacity (mirroring the
            // original's own memmove) is safe.
            unsafe {
                let ga_data = &mut (*b).bv_ga.ga_data;
                for i in (before..len).rev() {
                    ga_data[(i + 1) as usize] = ga_data[i as usize];
                }
                ga_data[before as usize] = val as u8;
                (*b).bv_ga.ga_len += 1;
            }
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_copy(&argvars[0], rettv) };
        }
        TypvalValue::List(_) => {
            // SAFETY: forwarded from this function's own safety doc.
            let TypvalValue::List(mut l) = argvars[0].value else { unreachable!() };
            // SAFETY: forwarded from this function's own safety doc.
            let locked = unsafe { crate::eval::typval::tv_list_locked(l) };
            if crate::eval::typval::value_check_lock(locked, None) {
                return;
            }

            let mut before: crate::eval::typval_defs::VarnumberT = 0;
            if argvars.len() > 2 {
                let mut error = false;
                before = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error));
                if error {
                    return;
                }
            }

            let mut item = std::ptr::null_mut();
            // SAFETY: forwarded from this function's own safety doc.
            if before != i64::from(unsafe { crate::eval::typval::tv_list_len(l) }) {
                // SAFETY: forwarded from this function's own safety doc.
                item = unsafe { crate::eval::typval::tv_list_find(l, before as i32) };
                if item.is_null() {
                    l = std::ptr::null_mut(); // semsg(_(e_list_index_out_of_range_nr), ...) omitted.
                }
            }
            if !l.is_null() {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    crate::eval::typval::tv_list_insert_tv(l, &argvars[1], item);
                    crate::eval::typval::tv_copy(&argvars[0], rettv);
                }
            }
        }
        _ => {
            // semsg(_(e_listblobarg), "insert()") omitted - message
            // display, not tractable; rettv is simply left at its
            // caller-provided default.
        }
    }
}

/// `remove({object}, {idx} [, {end}])` - remove an item (or a range)
/// from a `List`/`Blob` at `{idx}` (`{end}`, if given), or
/// `remove({dict}, {key})` - remove the entry `{key}` from a `Dict`
/// (`f_remove`, `eval/list.c`). Dispatches to
/// [`crate::eval::typval::tv_list_remove`]/
/// [`crate::eval::typval::tv_blob_remove`]/
/// [`crate::eval::typval::tv_dict_remove`] by `argvars[0]`'s own type -
/// every other type leaves `rettv` at its caller-provided default
/// (the original's own `semsg(_(e_listdictblobarg), "remove()")` is
/// omitted, message display not tractable).
///
/// # Safety
/// If `argvars[0].value` is `List`/`Dict`/`Blob`-typed with a
/// non-null pointer, that pointer must be valid, with every item
/// genuinely allocated via the matching `_alloc` helper.
unsafe fn f_remove(argvars: &[TypvalT], rettv: &mut TypvalT) {
    match &argvars[0].value {
        TypvalValue::Dict(_) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_dict_remove(argvars, rettv) };
        }
        TypvalValue::Blob(_) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_blob_remove(argvars, rettv) };
        }
        TypvalValue::List(_) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_list_remove(argvars, rettv) };
        }
        _ => {}
    }
}

/// The `List` case of `extend()`/`extendnew()` (`extend_list`,
/// `eval/list.c`).
///
/// `is_new` selects `extendnew()`'s own "copy first" behavior
/// (`true`) vs. `extend()`'s own in-place mutation (`false`) - when
/// `true`, `l1` is [`crate::eval::typval::tv_list_copy`]'d before
/// extending, and the copy is moved DIRECTLY into `rettv` (no extra
/// [`crate::eval::typval::tv_copy`]-based refcount increment, since
/// `tv_list_copy` already set the new list's own refcount to `1`,
/// exactly matching the original's own direct `rettv->vval.v_list =
/// l1` field assignment).
///
/// # Safety
/// `argvars[0]`/`argvars[1]`'s values must be `List`-typed with valid
/// pointers (non-null, or null - a null `argvars[1]` is a no-op,
/// matching the original's own `l2 == NULL` early-return).
unsafe fn extend_list(argvars: &[TypvalT], is_new: bool, rettv: &mut TypvalT) {
    let TypvalValue::List(mut l1) = argvars[0].value else { unreachable!() };

    // SAFETY: forwarded from this function's own safety doc.
    if !is_new && crate::eval::typval::value_check_lock(unsafe { crate::eval::typval::tv_list_locked(l1) }, None) {
        return;
    }

    if is_new {
        // SAFETY: forwarded from this function's own safety doc.
        l1 =
            unsafe { crate::eval::typval::tv_list_copy(std::ptr::null(), l1, false, crate::eval::eval::get_copy_id()) };
        if l1.is_null() {
            return;
        }
    }

    let TypvalValue::List(l2) = argvars[1].value else { unreachable!() };
    if !l2.is_null() {
        let mut item = std::ptr::null_mut();
        // argvars.len() > 2 replaces the original's own
        // argvars[2].v_type != VAR_UNKNOWN sentinel check.
        if argvars.len() > 2 {
            let mut error = false;
            let before = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error));
            if error {
                if is_new {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { crate::eval::typval::tv_list_unref(l1) };
                }
                return; // type error; errmsg already given in the original.
            }
            // SAFETY: forwarded from this function's own safety doc.
            if before != i64::from(unsafe { crate::eval::typval::tv_list_len(l1) }) {
                // SAFETY: forwarded from this function's own safety doc.
                item = unsafe { crate::eval::typval::tv_list_find(l1, before as i32) };
                if item.is_null() {
                    if is_new {
                        // SAFETY: forwarded from this function's own safety doc.
                        unsafe { crate::eval::typval::tv_list_unref(l1) };
                    }
                    return; // semsg(_(e_list_index_out_of_range_nr), ...) omitted.
                }
            }
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::typval::tv_list_extend(l1, l2, item) };
    }

    if is_new {
        rettv.value = TypvalValue::List(l1);
        rettv.v_lock = crate::eval::typval_defs::VarLockStatus::Unlocked;
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::typval::tv_copy(&argvars[0], rettv) };
    }
}

/// The `Dict` case of `extend()`/`extendnew()` (`extend_dict`,
/// `eval/list.c`). See [`extend_list`]'s own doc comment for the
/// shared `is_new` reasoning (identical here, for `Dict` instead of
/// `List`).
///
/// # Safety
/// `argvars[0]`/`argvars[1]`'s values must be `Dict`-typed with valid
/// pointers.
unsafe fn extend_dict(argvars: &[TypvalT], is_new: bool, rettv: &mut TypvalT) {
    let TypvalValue::Dict(mut d1) = argvars[0].value else { unreachable!() };
    if d1.is_null() {
        // The original's own value_check_lock(VAR_FIXED, ...) call
        // here always returns true (VAR_FIXED is always "locked") -
        // its only real effect is the error message it would display
        // (omitted, see this module's own doc comment), so a null d1
        // always just returns, unconditionally.
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    if !is_new && crate::eval::typval::value_check_lock(unsafe { (*d1).dv_lock }, None) {
        return;
    }

    if is_new {
        // SAFETY: forwarded from this function's own safety doc.
        d1 =
            unsafe { crate::eval::typval::tv_dict_copy(std::ptr::null(), d1, false, crate::eval::eval::get_copy_id()) };
        if d1.is_null() {
            return;
        }
    }

    let TypvalValue::Dict(d2) = argvars[1].value else { unreachable!() };
    if !d2.is_null() {
        let mut action: Vec<u8> = b"force".to_vec();
        if argvars.len() > 2 {
            let Some(action_str) = crate::eval::typval::tv_get_string_chk(&argvars[2]) else {
                if is_new {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { crate::eval::typval::tv_dict_unref(d1) };
                }
                return; // type error; errmsg already given in the original.
            };
            if !matches!(action_str.as_slice(), b"keep" | b"force" | b"error") {
                if is_new {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { crate::eval::typval::tv_dict_unref(d1) };
                }
                return; // semsg(_(e_invarg2), ...) omitted.
            }
            action = action_str;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::typval::tv_dict_extend(d1, d2, &action) };
    }

    if is_new {
        rettv.value = TypvalValue::Dict(d1);
        rettv.v_lock = crate::eval::typval_defs::VarLockStatus::Unlocked;
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::typval::tv_copy(&argvars[0], rettv) };
    }
}

/// Shared `extend()`/`extendnew()` dispatch by `argvars[0]`/`[1]`'s
/// own types (`extend`, `eval/list.c`'s own private static helper).
///
/// # Safety
/// Forwards [`extend_list`]/[`extend_dict`]'s own safety docs.
unsafe fn extend(argvars: &[TypvalT], is_new: bool, rettv: &mut TypvalT) {
    match (&argvars[0].value, &argvars[1].value) {
        // SAFETY: forwarded from this function's own safety doc.
        (TypvalValue::List(_), TypvalValue::List(_)) => unsafe { extend_list(argvars, is_new, rettv) },
        // SAFETY: forwarded from this function's own safety doc.
        (TypvalValue::Dict(_), TypvalValue::Dict(_)) => unsafe { extend_dict(argvars, is_new, rettv) },
        _ => {
            // semsg(_(e_listdictarg), is_new ? "extendnew()" : "extend()")
            // omitted - message display, not tractable; rettv is
            // simply left at its caller-provided default.
        }
    }
}

/// `extend({expr1}, {expr2} [, {expr3}])` - extend `{expr1}` (a `List`
/// or `Dict`) in place with `{expr2}`'s own items (`f_extend`,
/// `eval/list.c`). Returns the SAME (mutated) `{expr1}`.
///
/// # Safety
/// Forwards [`extend`]'s own safety doc.
unsafe fn f_extend(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { extend(argvars, false, rettv) };
}

/// `extendnew({expr1}, {expr2} [, {expr3}])` - like [`f_extend`], but
/// `{expr1}` is copied first, leaving the original untouched
/// (`f_extendnew`, `eval/list.c`). Returns the NEW, extended copy.
///
/// # Safety
/// Forwards [`extend`]'s own safety doc.
unsafe fn f_extendnew(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { extend(argvars, true, rettv) };
}

/// `range({expr} [, {max} [, {stride}]])` - a `List` of numbers
/// (`f_range`, `funcs.c`).
///
/// One argument: `[0, 1, ..., {expr} - 1]`. Two: `[{expr}, {expr} + 1,
/// ..., {max}]`. Three: like two, but stepping by `{stride}` each time
/// (which may be negative, counting down).
///
/// Every error path (a type error reading any argument, a zero
/// `{stride}`, or `{start}` past `{end}` for the given `{stride}`'s
/// own direction) is a bare early return leaving `rettv` at its
/// caller-provided default - the original's own `emsg` calls are
/// omitted, message display not tractable (see this module's own doc
/// comment).
unsafe fn f_range(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let mut error = false;
    let mut start = crate::eval::typval::tv_get_number_chk(&argvars[0], Some(&mut error));
    let mut stride: crate::eval::typval_defs::VarnumberT = 1;
    let end;
    // argvars.len() > 1 replaces the original's own argvars[1].v_type
    // != VAR_UNKNOWN sentinel check.
    if argvars.len() <= 1 {
        end = start - 1;
        start = 0;
    } else {
        end = crate::eval::typval::tv_get_number_chk(&argvars[1], Some(&mut error));
        if argvars.len() > 2 {
            stride = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error));
        }
    }

    if error {
        return; // type error; errmsg already given in the original.
    }
    if stride == 0 {
        return; // emsg(_("E726: Stride is zero")) omitted.
    }
    if if stride > 0 { end + 1 < start } else { end - 1 > start } {
        return; // emsg(_("E727: Start past end")) omitted.
    }

    // SAFETY: rettv is a plain `&mut TypvalT`, always safe to write
    // into; tv_list_alloc_ret's own `len` hint is unused (see its own
    // doc comment), so an oddly-signed `(end - start) / stride` here
    // is harmless either way.
    let list = unsafe { crate::eval::typval::tv_list_alloc_ret(rettv, ((end - start) / stride) as isize) };
    let mut i = start;
    while if stride > 0 { i <= end } else { i >= end } {
        // SAFETY: `list` was just allocated above by this same call.
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, i) };
        i += stride;
    }
}

/// Repeat list `l` `n` times into a NEW list, set into `rettv`
/// (`repeat_list`, `funcs.c`).
///
/// # Safety
/// `l`, if non-null, must be a valid pointer to a live
/// [`crate::eval::typval_defs::ListT`].
unsafe fn repeat_list(l: *mut crate::eval::typval_defs::ListT, n: crate::eval::typval_defs::VarnumberT, rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    let len_hint = if n > 0 { n * i64::from(unsafe { crate::eval::typval::tv_list_len(l) }) } else { 0 };
    // SAFETY: rettv is a plain `&mut TypvalT`, always safe to write
    // into.
    let new_list = unsafe { crate::eval::typval::tv_list_alloc_ret(rettv, len_hint as isize) };
    let mut remaining = n;
    while remaining > 0 {
        remaining -= 1;
        // SAFETY: `new_list` was just allocated above by this same
        // call; `l`, forwarded from this function's own safety doc.
        unsafe { crate::eval::typval::tv_list_extend(new_list, l, std::ptr::null_mut()) };
    }
}

/// Repeat blob `blob_tv`'s own bytes `n` times into a NEW blob, set
/// into `rettv` (`repeat_blob`, `funcs.c`).
///
/// Builds the repeated byte buffer directly via a plain `Vec<u8>`
/// (`bv_ga.ga_data` IS one in this crate) rather than the original's
/// own `ga_grow`+`tv_blob_set_range`-based approach (including its
/// own "skip the copy if every byte is already zero" optimization,
/// a pure C-level micro-optimization not needed here) - `funcs.c`'s
/// own general-purpose `tv_blob_set_range` (used elsewhere for list-
/// index-assignment into a blob, not yet translated) isn't needed for
/// this one, simpler, always-appending caller.
///
/// # Safety
/// `blob_tv.value` must be `Blob`-typed; if its pointer is non-null,
/// it must be valid.
unsafe fn repeat_blob(blob_tv: &TypvalT, n: crate::eval::typval_defs::VarnumberT, rettv: &mut TypvalT) {
    let TypvalValue::Blob(blob) = blob_tv.value else { unreachable!() };

    let new_blob = crate::eval::typval::tv_blob_alloc_ret(rettv);
    if blob.is_null() || n <= 0 {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let slen = unsafe { crate::eval::typval::tv_blob_len(blob) };
    let len = i64::from(slen) * n;
    if len <= 0 {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let src: Vec<u8> = unsafe { (&(*blob).bv_ga.ga_data)[..slen as usize].to_vec() };
    let mut data = Vec::with_capacity(len as usize);
    for _ in 0..n {
        data.extend_from_slice(&src);
    }
    // SAFETY: `new_blob` was just allocated above, a fresh pointer
    // not shared with anything yet.
    unsafe {
        (*new_blob).bv_ga.ga_data = data;
        (*new_blob).bv_ga.ga_len = len as i32;
        (*new_blob).bv_ga.ga_maxlen = len as i32;
    }
}

/// Repeat string `str_tv`'s own bytes `n` times into a NEW string, set
/// into `rettv` (`repeat_string`, `funcs.c`).
///
/// `strlen(p)` (the original's own C-string-bounded scan) is mirrored
/// by stopping at the first embedded NUL (or the full length, if
/// none) - this crate's own established "embedded NUL ends a
/// C-string-modeled scan" idiom, used throughout this module.
/// Overflow detection uses [`usize::checked_mul`] directly (a more
/// robust equivalent of the original's own `len / n != slen`
/// division-based check).
fn repeat_string(str_tv: &TypvalT, n: crate::eval::typval_defs::VarnumberT, rettv: &mut TypvalT) {
    rettv.value = TypvalValue::String(None);
    if n <= 0 {
        return;
    }

    let s = crate::eval::typval::tv_get_string(str_tv);
    let slen = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    if slen == 0 {
        return;
    }
    let Some(len) = slen.checked_mul(n as usize) else {
        return; // overflow.
    };

    let mut r = Vec::with_capacity(len);
    for _ in 0..n {
        r.extend_from_slice(&s[..slen]);
    }
    rettv.value = TypvalValue::String(Some(r));
}

/// `repeat({expr}, {count})` - repeat `{expr}` `{count}` times,
/// concatenated (`f_repeat`, `funcs.c`). `{expr}` may be a `List`
/// (dispatches to [`repeat_list`]), `Blob` ([`repeat_blob`]), or
/// anything else, stringified first ([`repeat_string`]).
///
/// # Safety
/// If `argvars[0].value` is `List`/`Blob`-typed with a non-null
/// pointer, that pointer must be valid.
unsafe fn f_repeat(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let n = crate::eval::typval::tv_get_number(&argvars[1]);
    match &argvars[0].value {
        TypvalValue::List(l) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { repeat_list(*l, n, rettv) };
        }
        TypvalValue::Blob(_) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { repeat_blob(&argvars[0], n, rettv) };
        }
        _ => repeat_string(&argvars[0], n, rettv),
    }
}

/// `join({list} [, {sep}])` - join `{list}`'s own items into a single
/// string, separated by `{sep}` (default `" "`) (`f_join`,
/// `eval/typval.c`).
///
/// A non-`List` `argvars[0]` is a bare early return leaving `rettv` at
/// its caller-provided default (the original's own `emsg(_(e_listreq))`
/// is omitted). A `{sep}` type error still sets `rettv` to a null
/// `String` (not left at the caller default) - matching the
/// original's own unconditional `rettv->v_type = VAR_STRING;` once
/// `argvars[0]` is confirmed to be a real `List`, regardless of
/// whether `{sep}` itself was valid.
///
/// # Safety
/// Forwards [`crate::eval::typval::tv_list_join`]'s own safety
/// doc/panic condition.
unsafe fn f_join(argvars: &[TypvalT], rettv: &mut TypvalT) {
    if !matches!(argvars[0].value, TypvalValue::List(_)) {
        return; // emsg(_(e_listreq)) omitted; rettv left at its caller-provided default.
    }
    let TypvalValue::List(l) = argvars[0].value else { unreachable!() };

    let sep: Vec<u8> = if argvars.len() > 1 {
        let Some(s) = crate::eval::typval::tv_get_string_chk(&argvars[1]) else {
            rettv.value = TypvalValue::String(None);
            return; // type error; errmsg already given in the original.
        };
        s
    } else {
        b" ".to_vec()
    };

    // SAFETY: forwarded from this function's own safety doc.
    let joined = unsafe { crate::eval::typval::tv_list_join(l, &sep) };
    rettv.value = TypvalValue::String(Some(joined));
}

/// Shared `flatten()`/`flattennew()` implementation (`flatten_common`,
/// `funcs.c`). `make_copy` selects `flattennew()`'s own "copy first"
/// behavior (`true`) vs. `flatten()`'s own in-place mutation
/// (`false`) - matching [`extend_list`]'s own identical `is_new`
/// reasoning (though here, unlike `extend_list`, `rettv`'s own List
/// value is assigned UPFRONT, before flattening even begins, exactly
/// matching the original's own statement order).
///
/// # Safety
/// If `argvars[0].value` is `List`-typed with a non-null pointer, it
/// must be valid, recursively (forwarded to
/// [`crate::eval::typval::tv_list_flatten`]'s own safety doc).
unsafe fn flatten_common(argvars: &[TypvalT], make_copy: bool, rettv: &mut TypvalT) {
    if !matches!(argvars[0].value, TypvalValue::List(_)) {
        return; // semsg(_(e_listarg), "flatten()") omitted; rettv left at its caller-provided default.
    }

    let mut maxdepth: i64 = 999_999;
    // argvars.len() > 1 replaces the original's own argvars[1].v_type
    // != VAR_UNKNOWN sentinel check.
    if argvars.len() > 1 {
        let mut error = false;
        maxdepth = crate::eval::typval::tv_get_number_chk(&argvars[1], Some(&mut error));
        if error {
            return; // type error; errmsg already given in the original.
        }
        if maxdepth < 0 {
            return; // emsg(_("E900: maxdepth must be non-negative number")) omitted.
        }
    }

    let TypvalValue::List(mut list) = argvars[0].value else { unreachable!() };
    rettv.value = TypvalValue::List(list);
    if list.is_null() {
        return;
    }

    if make_copy {
        // SAFETY: forwarded from this function's own safety doc.
        list = unsafe {
            crate::eval::typval::tv_list_copy(std::ptr::null(), list, false, crate::eval::eval::get_copy_id())
        };
        rettv.value = TypvalValue::List(list);
        if list.is_null() {
            return;
        }
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        if crate::eval::typval::value_check_lock(unsafe { crate::eval::typval::tv_list_locked(list) }, None) {
            return;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::typval::tv_list_ref(list) };
    }

    // SAFETY: forwarded from this function's own safety doc.
    let len = unsafe { crate::eval::typval::tv_list_len(list) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::typval::tv_list_flatten(list, std::ptr::null_mut(), i64::from(len), maxdepth) };
}

/// `flatten({list} [, {maxdepth}])` - flatten `{list}` in place, up to
/// `{maxdepth}` levels (default effectively unlimited) (`f_flatten`,
/// `funcs.c`). Returns the SAME (mutated) `{list}`.
///
/// # Safety
/// Forwards [`flatten_common`]'s own safety doc.
unsafe fn f_flatten(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { flatten_common(argvars, false, rettv) };
}

/// `flattennew({list} [, {maxdepth}])` - like [`f_flatten`], but
/// `{list}` is copied first, leaving the original untouched
/// (`f_flattennew`, `funcs.c`). Returns the NEW, flattened copy.
///
/// # Safety
/// Forwards [`flatten_common`]'s own safety doc.
unsafe fn f_flattennew(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { flatten_common(argvars, true, rettv) };
}

/// `localtime()` - the current time, measured in seconds since 1970-01-01
/// (`f_localtime`, `funcs.c`).
fn f_localtime(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(crate::os::time::os_time() as crate::eval::typval_defs::VarnumberT);
}

/// `getenv({name})` - the value of environment variable `{name}`, or
/// `v:null` if unset (`f_getenv`, `funcs.c`), via the already-existing
/// [`crate::os::env::vim_getenv`].
///
/// # Safety
/// Forwards `vim_getenv`'s own safety doc (Windows `$HOME` path only).
/// Also panics if `{name}` is `"VIM"`/`"VIMRUNTIME"` and that variable
/// isn't ACTUALLY set in the real environment - `vim_getenv`'s own,
/// pre-existing, already-documented gap (its `$VIM`/`$VIMRUNTIME`
/// runtime-directory auto-discovery fallback isn't translated yet),
/// not something this function itself introduces.
unsafe fn f_getenv(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let name = crate::eval::typval::tv_get_string(&argvars[0]);
    // SAFETY: forwarded from this function's own safety doc.
    match unsafe { crate::os::env::vim_getenv(&name) } {
        Some(value) => rettv.value = TypvalValue::String(Some(value)),
        None => rettv.value = TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null),
    }
}

/// `environ()` - all environment variables as a `Dict` (`f_environ`).
///
/// Unlike every other builtin in this module, this one has NO C
/// implementation at all - the original itself implements it in Lua
/// (`runtime/lua/vim/_core/vimfn.lua`'s own `M.f_environ`), calling
/// `vim.uv.os_environ()` (libuv's own environment-enumeration) and,
/// on Windows only, force-uppercasing every key (matching legacy Vim
/// behavior, `#39443`). Translated directly from that Lua source
/// (this whole mission's own scope explicitly includes Neovim's Lua
/// source, not just its C source) using `std::env::vars_os()` as the
/// portable equivalent of `os_environ()` - lossy-UTF-8-converted to
/// `Vec<u8>`, matching [`crate::os::env::os_getenv`]'s own already-
/// established conversion for the exact same underlying `OsString`
/// values.
fn f_environ(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    let d = crate::eval::typval::tv_dict_alloc();
    for (key, val) in std::env::vars_os() {
        let mut key = key.to_string_lossy().into_owned();
        if cfg!(windows) {
            key = key.to_uppercase();
        }
        let val = val.to_string_lossy().into_owned();
        // SAFETY: `d` was just allocated above, a fresh pointer not
        // shared with anything yet.
        unsafe { crate::eval::typval::tv_dict_add_str(&mut *d, key.as_bytes(), Some(val.as_bytes())) };
    }
    // SAFETY: `d` was just allocated above, a fresh pointer not
    // shared with anything yet.
    unsafe { crate::eval::typval::tv_dict_set_ret(rettv, d) };
}

/// `setenv({name}, {val})` - set environment variable `{name}` to
/// `{val}` (`{val}` == `v:null` deletes it instead) (`f_setenv`,
/// `funcs.c`), via the already-existing
/// [`crate::os::env::vim_setenv_ext`]/
/// [`crate::os::env::vim_unsetenv_ext`]. No return value (`rettv`
/// stays whatever the caller pre-initialized it to), matching the
/// original's own `void`-typed implementation.
///
/// # Safety
/// Touches `crate::globals::GLOBALS` (via
/// [`crate::ex_cmds::check_secure`] and `vim_setenv_ext`/
/// `vim_unsetenv_ext` themselves).
unsafe fn f_setenv(argvars: &[TypvalT], _rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::ex_cmds::check_secure() } {
        return;
    }

    let name = crate::eval::typval::tv_get_string(&argvars[0]);
    if matches!(argvars[1].value, TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null)) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::os::env::vim_unsetenv_ext(&name) };
    } else {
        let val = crate::eval::typval::tv_get_string(&argvars[1]);
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::os::env::vim_setenv_ext(&name, &val) };
    }
}

/// `changenr()` - the number of the most recent change (`f_changenr`,
/// `funcs.c`), directly reading `curbuf.b_u_seq_cur`. `0` when the
/// undo list is empty.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
unsafe fn f_changenr(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    rettv.value = TypvalValue::Number(i64::from(curbuf.b_u_seq_cur));
}

/// `interrupt()` - simulate a user-typed CTRL-C, aborting script
/// execution (`f_interrupt`, `funcs.c`), by setting the already-real
/// `GLOBALS.got_int`.
///
/// # Safety
/// Touches `crate::globals::GLOBALS`.
unsafe fn f_interrupt(_argvars: &[TypvalT], _rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::globals::GLOBALS.get_mut() }.got_int = true;
}

/// `invert({expr})` - bitwise NOT of `{expr}` converted to a Number
/// (`f_invert`, `funcs.c`).
fn f_invert(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(!crate::eval::typval::tv_get_number_chk(&argvars[0], None));
}

/// `getfontname([{name}])` - GUI font name (`f_getfontname`, `funcs.c`).
/// This crate never runs a GUI, so this always returns the original's
/// own "GUI not running" result: an empty (`None`-backed) `String`,
/// matching the real implementation's unconditional
/// `rettv->vval.v_string = NULL` (it doesn't even inspect `argvars`).
fn f_getfontname(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::String(None);
}

/// `isinf({expr})` - `1` for positive infinity, `-1` for negative
/// infinity, `0` otherwise (`f_isinf`, `funcs.c`).
fn f_isinf(argvars: &[TypvalT], rettv: &mut TypvalT) {
    if let TypvalValue::Float(f) = argvars[0].value {
        if f.is_infinite() {
            rettv.value = TypvalValue::Number(if f > 0.0 { 1 } else { -1 });
        }
    }
}

/// `isnan({expr})` - whether `{expr}` is a Float with value NaN
/// (`f_isnan`, `funcs.c`).
fn f_isnan(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let is_nan = matches!(argvars[0].value, TypvalValue::Float(f) if f.is_nan());
    rettv.value = TypvalValue::Number(i64::from(is_nan));
}

/// `sha256({expr})` - the SHA256 checksum of `{expr}` (a String or a
/// Blob), as 64 hex characters (`f_sha256`, `funcs.c`), via the
/// already-existing [`crate::sha256::sha256_bytes`].
///
/// # Safety
/// If `{expr}` is a Blob, its pointer must be valid (matching every
/// other function in this crate that touches `TypvalValue::Blob`).
unsafe fn f_sha256(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let hash = if let TypvalValue::Blob(b) = argvars[0].value {
        // SAFETY: forwarded from this function's own safety doc.
        let bytes = unsafe { b.as_ref() }.map_or(&[][..], |blob| {
            let len = usize::try_from(blob.bv_ga.ga_len).unwrap_or(0).min(blob.bv_ga.ga_data.len());
            &blob.bv_ga.ga_data[..len]
        });
        crate::sha256::sha256_bytes(bytes, None)
    } else {
        let s = crate::eval::typval::tv_get_string(&argvars[0]);
        crate::sha256::sha256_bytes(&s, None)
    };
    rettv.value = TypvalValue::String(Some(hash.into_bytes()));
}

/// `exists({expr})` - check the existence of various kinds of things,
/// dispatched on `{expr}`'s own leading character (`f_exists`,
/// `funcs.c`).
///
/// Only 2 of the original's 6 branches are translated:
/// - `&option`/`+option`: fully faithful, via the already-existing
///   [`crate::eval::eval::eval_option`] (called with `rettv = None`,
///   matching the original's own "just check existence" idiom) plus
///   the same "no trailing garbage after the name" check.
/// - the default "plain variable name" case: fully faithful, via the
///   newly-translated [`crate::eval::vars::var_exists`].
///
/// `$env` is a DELIBERATE, narrower gap (not a panic): only the
/// original's own fast path is modeled (a literal, already-set
/// environment variable, via the already-existing
/// [`crate::os::env::os_env_exists`]) - the original's fallback
/// (`expand_env_save`, a substantial general-purpose `$VAR`/`${VAR}`/
/// `~`/`` `=expr` ``-in-the-middle-of-an-arbitrary-string expander,
/// used throughout the original for path expansion generally, not
/// just this one narrow check) is NOT modeled, so this returns
/// `false` rather than `true` for the rare case of a variable only
/// resolvable through that indirect machinery (e.g. `$VIM`/
/// `$VIMRUNTIME` when not literally exported, the same already-
/// accepted gap `vim_getenv`/`f_getenv` themselves have) - chosen
/// deliberately over panicking, since `exists()` is overwhelmingly
/// used defensively in real scripts specifically to AVOID errors.
///
/// `*func` (needs `function_exists`/`nlua_func_exists`), `:cmd`
/// (needs `cmd_exists`), and `#autocmd`/`##event` (needs `au_exists`/
/// `autocmd_supported`) each need a substantial not-yet-translated
/// subsystem and panic via `unimplemented!()` if actually reached -
/// matching this crate's established "translate the common path,
/// panic loudly on a genuinely untranslated-but-reached path"
/// convention (unlike `$env` above, none of these have an existing,
/// already-tractable fast path to fall back on).
///
/// # Safety
/// Forwards [`crate::eval::vars::var_exists`]'s own safety doc for the
/// default branch, and [`crate::eval::eval::eval_option`]'s own safety
/// doc for the `&`/`+` branch.
unsafe fn f_exists(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let p = crate::eval::typval::tv_get_string(&argvars[0]);
    let n = match p.first() {
        Some(b'$') => crate::os::env::os_env_exists(&p[1..], false),
        Some(b'&' | b'+') => {
            // SAFETY: forwarded from this function's own safety doc.
            let (status, consumed) = unsafe { crate::eval::eval::eval_option(&p, None, true) };
            let rest = &p[consumed.min(p.len())..];
            let ws = crate::charset::skipwhite(rest);
            status == crate::vim_defs::OK && rest[ws..].is_empty()
        }
        Some(b'*') => {
            unimplemented!("exists(): '*func' branch needs function_exists/nlua_func_exists, not yet translated");
        }
        Some(b':') => {
            unimplemented!("exists(): ':cmd' branch needs cmd_exists, not yet translated");
        }
        Some(b'#') => {
            unimplemented!("exists(): '#autocmd'/'##event' branch needs au_exists/autocmd_supported, not yet translated");
        }
        _ => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::vars::var_exists(&p) }
        }
    };
    rettv.value = TypvalValue::Number(i64::from(n));
}

/// `getwinpos([{timeout}])` - the `[x, y]` screen position of the
/// Nvim GUI window (`f_getwinpos`, `eval/window.c`). This crate never
/// runs a GUI, so this always returns the original's own "not
/// available" result: `[-1, -1]` (the real implementation
/// unconditionally returns this, ignoring `{timeout}` entirely).
///
/// # Safety
/// Forwarded from [`crate::eval::typval::tv_list_alloc_ret`]'s own
/// safety doc.
unsafe fn f_getwinpos(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    let l = unsafe { crate::eval::typval::tv_list_alloc_ret(rettv, 2) };
    // SAFETY: `l` was just allocated above by this same call.
    unsafe {
        crate::eval::typval::tv_list_append_number(l, -1);
        crate::eval::typval::tv_list_append_number(l, -1);
    }
}

/// `getwinposx()` - the X coordinate of the Nvim GUI window, or `-1`
/// if not available (`f_getwinposx`, `eval/window.c`). Always `-1`
/// here - see [`f_getwinpos`]'s own doc comment (no GUI ever runs in
/// this crate).
fn f_getwinposx(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(-1);
}

/// `getwinposy()` - the Y coordinate of the Nvim GUI window, or `-1`
/// if not available (`f_getwinposy`, `eval/window.c`). Always `-1`
/// here - see [`f_getwinpos`]'s own doc comment (no GUI ever runs in
/// this crate).
fn f_getwinposy(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(-1);
}

/// `win_getid([{win} [, {tab}]])` - the window-ID for window number
/// `{win}` in tab number `{tab}` (`f_win_getid`, `eval/window.c`), via
/// the already-existing [`crate::window::win_getid`].
///
/// # Safety
/// Forwarded from `crate::window::win_getid`'s own safety doc.
unsafe fn f_win_getid(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let winnr = if argvars.is_empty() { None } else { Some(crate::eval::typval::tv_get_number(&argvars[0]) as i32) };
    let tabnr = if argvars.len() < 2 { None } else { Some(crate::eval::typval::tv_get_number(&argvars[1]) as i32) };
    // SAFETY: forwarded from this function's own safety doc.
    rettv.value = TypvalValue::Number(i64::from(unsafe { crate::window::win_getid(winnr, tabnr) }));
}

/// `win_id2win({expr})` - the window-number (within the current tab
/// page) of window-ID `{expr}` (`f_win_id2win`, `eval/window.c`), via
/// the already-existing [`crate::window::win_id2win`].
///
/// # Safety
/// Forwarded from `crate::window::win_id2win`'s own safety doc.
unsafe fn f_win_id2win(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let id = crate::eval::typval::tv_get_number(&argvars[0]) as i32;
    // SAFETY: forwarded from this function's own safety doc.
    rettv.value = TypvalValue::Number(i64::from(unsafe { crate::window::win_id2win(id) }));
}

/// `win_id2tabwin({expr})` - `[tabnr, winnr]` for window-ID `{expr}`,
/// `[0, 0]` if not found (`f_win_id2tabwin`, `eval/window.c`), via the
/// already-existing [`crate::window::win_get_tabwin`].
///
/// # Safety
/// Forwards `crate::window::win_get_tabwin`'s own safety doc, plus
/// [`crate::eval::typval::tv_list_alloc_ret`]'s own safety doc.
unsafe fn f_win_id2tabwin(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let id = crate::eval::typval::tv_get_number(&argvars[0]) as i32;
    // SAFETY: forwarded from this function's own safety doc.
    let (tabnr, winnr) = unsafe { crate::window::win_get_tabwin(id) };
    // SAFETY: forwarded from this function's own safety doc.
    let l = unsafe { crate::eval::typval::tv_list_alloc_ret(rettv, 2) };
    // SAFETY: `l` was just allocated above by this same call.
    unsafe {
        crate::eval::typval::tv_list_append_number(l, i64::from(tabnr));
        crate::eval::typval::tv_list_append_number(l, i64::from(winnr));
    }
}

/// `win_findbuf({bufnr})` - a `List` of window-IDs (across all tab
/// pages) currently showing buffer `{bufnr}`, empty if none
/// (`f_win_findbuf`, `eval/window.c`), via the already-existing
/// [`crate::window::win_findbuf`].
///
/// # Safety
/// Forwards `crate::window::win_findbuf`'s own safety doc, plus
/// [`crate::eval::typval::tv_list_alloc_ret`]'s own safety doc.
unsafe fn f_win_findbuf(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let bufnr = crate::eval::typval::tv_get_number(&argvars[0]) as i32;
    // SAFETY: forwarded from this function's own safety doc.
    let handles = unsafe { crate::window::win_findbuf(bufnr) };
    // SAFETY: forwarded from this function's own safety doc.
    let l = unsafe { crate::eval::typval::tv_list_alloc_ret(rettv, handles.len() as isize) };
    for handle in handles {
        // SAFETY: `l` was just allocated above by this same call.
        unsafe { crate::eval::typval::tv_list_append_number(l, i64::from(handle)) };
    }
}

/// `winnr([{arg}])` - the number of the current window (`f_winnr`,
/// `eval/window.c`), via the already-existing
/// [`crate::window::get_winnr`], called with `GLOBALS.curtab`
/// (matching the original's own `get_winnr(curtab, &argvars[0])`).
///
/// # Safety
/// Forwarded from `crate::window::get_winnr`'s own safety doc.
unsafe fn f_winnr(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let arg = if argvars.is_empty() { None } else { Some(crate::eval::typval::tv_get_string(&argvars[0])) };
    // SAFETY: forwarded from this function's own safety doc.
    let curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
    // SAFETY: forwarded from this function's own safety doc.
    let n = unsafe { crate::window::get_winnr(curtab, arg.as_deref()) };
    rettv.value = TypvalValue::Number(i64::from(n));
}

/// The unconditional (platform-independent) entries of `funcs.c`'s own
/// `has_list[]` static array, used by [`f_has`] - compile-time feature
/// flags (this build supports capability X), not runtime state, hence
/// unconditional regardless of THIS crate's own current translation
/// progress on any given subsystem (matching the original's own
/// "always present" nature for these, e.g. `has('spell')` is `1` in
/// any real Nvim build even with `'spell'` off for the current
/// buffer).
const HAS_LIST_UNCONDITIONAL: &[&[u8]] = &[
    b"autochdir",
    b"arabic",
    b"autocmd",
    b"browsefilter",
    b"byte_offset",
    b"cindent",
    b"cmdline_compl",
    b"cmdline_hist",
    b"cmdwin",
    b"comments",
    b"conceal",
    b"cursorbind",
    b"cursorshape",
    b"dialog_con",
    b"diff",
    b"digraphs",
    b"eval",
    b"ex_extra",
    b"extra_search",
    b"file_in_path",
    b"filterpipe",
    b"find_in_path",
    b"float",
    b"folding",
    b"gettext",
    b"iconv",
    b"insert_expand",
    b"jumplist",
    b"keymap",
    b"lambda",
    b"langmap",
    b"libcall",
    b"linebreak",
    b"lispindent",
    b"listcmds",
    b"localmap",
    b"menu",
    b"mksession",
    b"modify_fname",
    b"mouse",
    b"multi_byte",
    b"multi_lang",
    b"nanotime",
    b"num64",
    b"packages",
    b"path_extra",
    b"persistent_undo",
    b"profile",
    b"reltime",
    b"quickfix",
    b"rightleft",
    b"scrollbind",
    b"showcmd",
    b"cmdline_info",
    b"shada",
    b"signs",
    b"smartindent",
    b"startuptime",
    b"statusline",
    b"spell",
    b"syntax",
    b"tablineat",
    b"tag_binary",
    b"termguicolors",
    b"termresponse",
    b"textobjects",
    b"timers",
    b"title",
    b"user-commands",
    b"user_commands",
    b"vartabs",
    b"vertsplit",
    b"vimscript-1",
    b"virtualedit",
    b"visual",
    b"visualextra",
    b"vreplace",
    b"wildignore",
    b"wildmenu",
    b"windows",
    b"winaltkeys",
    b"writebackup",
    b"nvim",
];

/// `has({feature})` - whether `{feature}` is supported (`f_has`,
/// `funcs.c`), case-insensitively.
///
/// Checks `HAS_LIST_UNCONDITIONAL` plus the original's own platform-
/// conditional (`#ifdef`) entries this crate CAN meaningfully
/// determine at compile time (`unix`/`linux`/`win32`/`win64`/`mac`
/// family/`fork`/`system`/`fname_case`).
///
/// Every other special case the original handles dynamically
/// (`patch-N`/`nvim-x.y.z` version checks, `vim_starting`/`ttyin`/
/// `ttyout`/`gui_running`/`syntax_items`/`wsl` runtime state, and
/// provider-based checks like `clipboard_working`/`pythonx`) simply
/// returns `0` here - not a translation gap that panics, since
/// `has()`'s own contract ("is this obscure feature present") already
/// naturally accommodates "not present" as a fully valid, non-error
/// answer for a real Nvim build too (e.g. `has('gui_running')` is `0`
/// in any terminal-only session, which describes EVERY session this
/// crate can currently produce anyway).
fn f_has(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let name = crate::eval::typval::tv_get_string(&argvars[0]);

    let platform_conditional: &[&[u8]] = if cfg!(windows) {
        &[&b"win32"[..], &b"system"[..]]
    } else if cfg!(target_os = "macos") {
        &[&b"unix"[..], &b"fork"[..], &b"mac"[..], &b"macunix"[..], &b"osx"[..], &b"osxdarwin"[..]]
    } else if cfg!(target_os = "linux") {
        &[&b"unix"[..], &b"fork"[..], &b"linux"[..], &b"fname_case"[..]]
    } else if cfg!(unix) {
        &[&b"unix"[..], &b"fork"[..]]
    } else {
        &[]
    };
    let win64_matches =
        cfg!(all(windows, target_pointer_width = "64")) && name.eq_ignore_ascii_case(b"win64");

    let found = win64_matches
        || platform_conditional.iter().any(|f| f.eq_ignore_ascii_case(&name))
        || HAS_LIST_UNCONDITIONAL.iter().any(|f| f.eq_ignore_ascii_case(&name));
    rettv.value = TypvalValue::Number(i64::from(found));
}

/// `strlen({string})` - the length of `{string}` in BYTES (`f_strlen`,
/// `strings.c`).
fn f_strlen(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    rettv.value = TypvalValue::Number(s.len() as i64);
}

/// Shared core of `strcharlen()`/`strchars()` (`strchar_common`,
/// `strings.c`): count characters, one advance at a time - composing
/// marks are folded into the base character (not counted separately)
/// when `skipcc` is `true`, matching
/// [`crate::mbyte::mb_ptr2char_adv`]; counted as their own separate
/// characters when `false`, matching
/// [`crate::mbyte::mb_cptr2char_adv`].
///
/// # Safety
/// Touches `OPTION_VARS` whenever `skipcc` is `true` (forwarded from
/// [`crate::mbyte::mb_ptr2char_adv`]'s own safety doc).
unsafe fn strchar_common(s: &[u8], skipcc: bool) -> i64 {
    let mut len = 0i64;
    let mut p = 0usize;
    while p < s.len() {
        let adv = if skipcc {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::mbyte::mb_ptr2char_adv(&s[p..]) }.1
        } else {
            crate::mbyte::mb_cptr2char_adv(&s[p..]).1
        };
        p += adv.max(1);
        len += 1;
    }
    len
}

/// `strcharlen({string})` - the number of characters in `{string}`,
/// composing characters ignored (`f_strcharlen`, `strings.c`) -
/// equivalent to `strchars({string}, 1)`.
///
/// # Safety
/// Forwarded from [`strchar_common`]'s own safety doc.
unsafe fn f_strcharlen(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    // SAFETY: forwarded from this function's own safety doc.
    rettv.value = TypvalValue::Number(unsafe { strchar_common(&s, true) });
}

/// `strchars({string} [, {skipcc}])` - the number of characters in
/// `{string}`; composing characters counted separately unless
/// `{skipcc}` is set (`f_strchars`, `strings.c`).
///
/// # Safety
/// Forwarded from [`strchar_common`]'s own safety doc.
unsafe fn f_strchars(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    let skipcc = argvars.len() > 1 && crate::eval::typval::tv_get_bool(&argvars[1]) != 0;
    // SAFETY: forwarded from this function's own safety doc.
    rettv.value = TypvalValue::Number(unsafe { strchar_common(&s, skipcc) });
}

/// `strwidth({string})` - the number of display cells `{string}`
/// occupies (`f_strwidth`, `strings.c`), via
/// [`crate::mbyte::mb_string2cells`].
///
/// # Safety
/// Forwarded from [`crate::mbyte::mb_string2cells`]'s own safety doc.
unsafe fn f_strwidth(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    // SAFETY: forwarded from this function's own safety doc.
    rettv.value = TypvalValue::Number(unsafe { crate::mbyte::mb_string2cells(&s) } as i64);
}

/// Byte-level `strstr()` equivalent: the first index where `needle`
/// occurs in `haystack`, or `None`. An empty `needle` matches at index
/// `0`, matching C's own `strstr("", "")`/`strstr(anything, "")`
/// contract.
fn find_substring(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// `stridx({haystack}, {needle} [, {start}])` - the first byte index
/// of `{needle}` in `{haystack}`, optionally starting the search at
/// byte `{start}` (though the reported index is always relative to
/// the very start of `{haystack}`), or `-1` if not found (`f_stridx`,
/// `strings.c`).
fn f_stridx(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(-1);
    let needle = crate::eval::typval::tv_get_string(&argvars[1]);
    let haystack = crate::eval::typval::tv_get_string(&argvars[0]);

    let mut search_start = 0usize;
    if argvars.len() > 2 {
        let start_idx = crate::eval::typval::tv_get_number(&argvars[2]);
        if start_idx >= haystack.len() as i64 {
            return;
        }
        if start_idx >= 0 {
            search_start = start_idx as usize;
        }
    }

    if let Some(pos) = find_substring(&haystack[search_start..], &needle) {
        rettv.value = TypvalValue::Number((search_start + pos) as i64);
    }
}

/// `strridx({haystack}, {needle} [, {start}])` - the LAST byte index
/// of `{needle}` in `{haystack}` at or before byte `{start}` (defaults
/// to the whole string), or `-1` if not found (`f_strridx`,
/// `strings.c`). An empty `{needle}` matches at `{start}` itself
/// (clamped into range), matching the original's own "empty string
/// matches past the end" behavior.
fn f_strridx(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(-1);
    let needle = crate::eval::typval::tv_get_string(&argvars[1]);
    let haystack = crate::eval::typval::tv_get_string(&argvars[0]);

    let end_idx = if argvars.len() > 2 {
        let n = crate::eval::typval::tv_get_number(&argvars[2]);
        if n < 0 {
            return;
        }
        n as usize
    } else {
        haystack.len()
    };

    if needle.is_empty() {
        rettv.value = TypvalValue::Number(end_idx.min(haystack.len()) as i64);
        return;
    }

    let mut last_match = None;
    let mut rest = 0usize;
    while rest <= haystack.len() {
        let Some(found) = find_substring(&haystack[rest..], &needle) else { break };
        let abs = rest + found;
        if abs > end_idx {
            break;
        }
        last_match = Some(abs);
        rest = abs + 1;
    }

    if let Some(m) = last_match {
        rettv.value = TypvalValue::Number(m as i64);
    }
}

/// `strgetchar({string}, {index})` - the character at (composing-
/// unaware) character index `{index}` of `{string}`, or `-1` if out
/// of range (`f_strgetchar`, `strings.c`).
fn f_strgetchar(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(-1);
    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    let mut charidx = crate::eval::typval::tv_get_number(&argvars[1]);

    let mut byteidx = 0usize;
    while charidx >= 0 && byteidx < s.len() {
        if charidx == 0 {
            rettv.value = TypvalValue::Number(crate::mbyte::utf_ptr2char(&s[byteidx..]) as i64);
            return;
        }
        charidx -= 1;
        byteidx += usize::try_from(crate::mbyte::utf_ptr2len(&s[byteidx..])).unwrap_or(1).max(1);
    }
}

/// `strpart({string}, {start} [, {len} [, {chars}]])` - a substring of
/// `{string}`, clamped to the overlap with the actual string
/// (`f_strpart`, `strings.c`). When `{chars}` is truthy, `{len}` (if
/// given) counts characters (via `utfc_ptr2len`) instead of bytes.
///
/// # Safety
/// Touches `OPTION_VARS` whenever `{chars}` is truthy (forwarded from
/// [`crate::mbyte::utfc_ptr2len`]'s own safety doc).
unsafe fn f_strpart(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let p = crate::eval::typval::tv_get_string(&argvars[0]);
    let slen = p.len() as i64;

    let n = crate::eval::typval::tv_get_number(&argvars[1]);
    let mut len = if argvars.len() > 2 { crate::eval::typval::tv_get_number(&argvars[2]) } else { slen - n };

    let mut n = n;
    if n < 0 {
        len += n;
        n = 0;
    } else if n > slen {
        n = slen;
    }
    if len < 0 {
        len = 0;
    } else if n + len > slen {
        len = slen - n;
    }

    if argvars.len() > 3 {
        let mut off = n;
        while off < slen && len > 0 {
            // SAFETY: forwarded from this function's own safety doc.
            off += i64::from(unsafe { crate::mbyte::utfc_ptr2len(&p[off as usize..]) });
            len -= 1;
        }
        len = off - n;
    }

    let n = n as usize;
    let len = len as usize;
    rettv.value = TypvalValue::String(Some(p[n..n + len].to_vec()));
}

/// `strtrans({string})` - `{string}` with every unprintable character
/// replaced by a printable representation (`f_strtrans`, `strings.c`),
/// via the already-translated [`crate::charset::transstr`] (`untab =
/// true`, matching the original's own call). `transstr` always
/// appends its own trailing NUL (matching this crate's line-storage
/// convention); stripped here since Vimscript `String` values carry
/// no such terminator.
///
/// # Safety
/// Forwarded from [`crate::charset::transstr`]'s own safety doc.
unsafe fn f_strtrans(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    // SAFETY: forwarded from this function's own safety doc.
    let mut out = unsafe { crate::charset::transstr(&s, true) };
    out.pop();
    rettv.value = TypvalValue::String(Some(out));
}

/// Shared core of `byteidx()`/`byteidxcomp()` (`byteidx_common`,
/// `strings.c`): the byte index of the `{nr}`th character of
/// `{string}`. `comp = true` (`byteidxcomp()`) counts composing
/// characters SEPARATELY (via [`crate::mbyte::utf_ptr2len`]);
/// `comp = false` (`byteidx()`) folds them into the preceding base
/// character (via [`crate::mbyte::utfc_ptr2len`]).
///
/// # Safety
/// Touches `OPTION_VARS` whenever `comp` is `false` (forwarded from
/// [`crate::mbyte::utfc_ptr2len`]'s own safety doc).
unsafe fn byteidx_common(argvars: &[TypvalT], rettv: &mut TypvalT, comp: bool) {
    rettv.value = TypvalValue::Number(-1);

    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    let mut idx = crate::eval::typval::tv_get_number(&argvars[1]);
    if idx < 0 {
        return;
    }

    let utf16idx = argvars.len() > 2 && crate::eval::typval::tv_get_bool(&argvars[2]) != 0;

    let mut t = 0usize;
    while idx > 0 {
        if t >= s.len() || s[t] == 0 {
            return;
        }
        let clen = if comp {
            crate::mbyte::utf_ptr2len(&s[t..])
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::mbyte::utfc_ptr2len(&s[t..]) }
        };
        if utf16idx {
            let c = if clen > 1 { crate::mbyte::utf_ptr2char(&s[t..]) } else { i32::from(s[t]) };
            if c > 0xFFFF {
                idx -= 1;
            }
            if idx > 0 {
                t += usize::try_from(clen).unwrap_or(1).max(1);
            }
        } else {
            t += usize::try_from(clen).unwrap_or(1).max(1);
        }
        idx -= 1;
    }
    rettv.value = TypvalValue::Number(t as i64);
}

/// `byteidx({string}, {nr} [, {utf16}])` - the byte index of the
/// `{nr}`th character of `{string}`, composing characters folded into
/// the preceding base character (`f_byteidx`, `strings.c`).
///
/// # Safety
/// Forwarded from [`byteidx_common`]'s own safety doc.
unsafe fn f_byteidx(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { byteidx_common(argvars, rettv, false) };
}

/// `byteidxcomp({string}, {nr} [, {utf16}])` - like [`f_byteidx`], but
/// composing characters are counted SEPARATELY (`f_byteidxcomp`,
/// `strings.c`).
///
/// # Safety
/// Forwarded from [`byteidx_common`]'s own safety doc.
unsafe fn f_byteidxcomp(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { byteidx_common(argvars, rettv, true) };
}

/// `charidx({string}, {idx} [, {countcc} [, {utf16}]])` - the
/// character index of the byte at `{idx}` in `{string}` (`f_charidx`,
/// `strings.c`).
///
/// # Safety
/// Touches `OPTION_VARS` whenever `{countcc}` is falsy (forwarded from
/// [`crate::mbyte::utfc_ptr2len`]'s own safety doc).
unsafe fn f_charidx(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(-1);

    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    let mut idx = crate::eval::typval::tv_get_number(&argvars[1]);
    if idx < 0 {
        return;
    }

    let mut countcc = false;
    let mut utf16idx = false;
    if argvars.len() > 2 {
        countcc = crate::eval::typval::tv_get_bool(&argvars[2]) != 0;
        if argvars.len() > 3 {
            utf16idx = crate::eval::typval::tv_get_bool(&argvars[3]) != 0;
        }
    }

    let mut p = 0usize;
    let mut len = 0i64;
    loop {
        let keep_going = if utf16idx { idx >= 0 } else { p as i64 <= idx };
        if !keep_going {
            break;
        }

        if p >= s.len() || s[p] == 0 {
            let matched = if utf16idx { idx == 0 } else { p as i64 == idx };
            if matched {
                rettv.value = TypvalValue::Number(len);
            }
            return;
        }

        let clen = if countcc {
            crate::mbyte::utf_ptr2len(&s[p..])
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::mbyte::utfc_ptr2len(&s[p..]) }
        };
        if utf16idx {
            idx -= 1;
            let c = if clen > 1 { crate::mbyte::utf_ptr2char(&s[p..]) } else { i32::from(s[p]) };
            if c > 0xFFFF {
                idx -= 1;
            }
        }
        p += usize::try_from(clen).unwrap_or(1).max(1);
        len += 1;
    }

    rettv.value = TypvalValue::Number(if len > 0 { len - 1 } else { 0 });
}

/// `strcharpart({src}, {start} [, {len} [, {skipcc}]])` - like
/// [`f_strpart`], but `{start}`/`{len}` count characters instead of
/// bytes (`f_strcharpart`, `strings.c`). `{skipcc}` (only consulted
/// when `{len}` is ALSO given) selects composing-aware
/// ([`crate::mbyte::utfc_ptr2len`]) vs. composing-separate
/// ([`crate::mbyte::utf_ptr2len`]) character widths, matching the
/// original's own exact gating.
///
/// # Safety
/// Touches `OPTION_VARS` whenever `{skipcc}` is truthy (forwarded from
/// [`crate::mbyte::utfc_ptr2len`]'s own safety doc).
unsafe fn f_strcharpart(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let p = crate::eval::typval::tv_get_string(&argvars[0]);
    let slen = p.len() as i64;

    let mut nbyte: i64 = 0;
    let mut skipcc = false;
    let mut error = false;
    let nchar = crate::eval::typval::tv_get_number_chk(&argvars[1], Some(&mut error));
    if !error {
        if argvars.len() > 3 {
            skipcc = crate::eval::typval::tv_get_bool(&argvars[3]) != 0;
        }

        if nchar > 0 {
            let mut nchar = nchar;
            while nchar > 0 && nbyte < slen {
                let clen = if skipcc {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { crate::mbyte::utfc_ptr2len(&p[nbyte as usize..]) }
                } else {
                    crate::mbyte::utf_ptr2len(&p[nbyte as usize..])
                };
                nbyte += i64::from(clen);
                nchar -= 1;
            }
        } else {
            nbyte = nchar;
        }
    }

    let mut len: i64;
    if argvars.len() > 2 {
        let mut charlen = crate::eval::typval::tv_get_number(&argvars[2]);
        len = 0;
        while charlen > 0 && nbyte + len < slen {
            let off = nbyte + len;
            if off < 0 {
                len += 1;
            } else {
                let clen = if skipcc {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { crate::mbyte::utfc_ptr2len(&p[off as usize..]) }
                } else {
                    crate::mbyte::utf_ptr2len(&p[off as usize..])
                };
                len += i64::from(clen);
            }
            charlen -= 1;
        }
    } else {
        len = slen - nbyte;
    }

    if nbyte < 0 {
        len += nbyte;
        nbyte = 0;
    } else if nbyte > slen {
        nbyte = slen;
    }
    if len < 0 {
        len = 0;
    } else if nbyte + len > slen {
        len = slen - nbyte;
    }

    let nbyte = nbyte as usize;
    let len = len as usize;
    rettv.value = TypvalValue::String(Some(p[nbyte..nbyte + len].to_vec()));
}

/// `getpid()` - the process ID of this Nvim process (`f_getpid`,
/// `funcs.c`), via the already-existing
/// [`crate::os::env::os_get_pid`].
fn f_getpid(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(crate::os::env::os_get_pid());
}

/// `tr({src}, {fromstr}, {tostr})` - `{src}` with every character
/// that appears in `{fromstr}` replaced by the character at the same
/// POSITION in `{tostr}` (`f_tr`, `strings.c`). A character not found
/// in `{fromstr}` is copied through unchanged. `{fromstr}`/`{tostr}`
/// must have the same number of (multi-byte) characters, and this is
/// checked exactly once: the first time a `{src}` character is found
/// that does NOT match `{fromstr}` (matching the original's own
/// `first`-gated check exactly, including its real quirk: if every
/// `{src}` character happens to match `{fromstr}`, the length
/// mismatch is never detected at all). On a real mismatch, the result
/// is an empty String (`None`), matching the original's own error
/// path (its `semsg` display omitted, matching this module's
/// established "skip the message, keep the state" policy).
///
/// # Safety
/// Touches `OPTION_VARS` (via [`crate::mbyte::utfc_ptr2len`]).
unsafe fn f_tr(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::String(None);

    let in_str = crate::eval::typval::tv_get_string(&argvars[0]);
    let fromstr = crate::eval::typval::tv_get_string(&argvars[1]);
    let tostr = crate::eval::typval::tv_get_string(&argvars[2]);

    // SAFETY: forwarded from this function's own safety doc.
    let char_len = |s: &[u8]| -> usize { usize::try_from(unsafe { crate::mbyte::utfc_ptr2len(s) }).unwrap_or(1).max(1) };

    let mut out = Vec::new();
    let mut first = true;
    let mut pos = 0usize;

    while pos < in_str.len() && in_str[pos] != 0 {
        let inlen = char_len(&in_str[pos..]);
        let inlen = inlen.min(in_str.len() - pos);
        let mut matched = false;
        let mut copy_from_to: Option<(usize, usize)> = None;
        let mut idx: i64 = 0;
        let mut fpos = 0usize;

        while fpos < fromstr.len() && fromstr[fpos] != 0 {
            let fromlen = char_len(&fromstr[fpos..]).min(fromstr.len() - fpos);
            if fromlen == inlen && in_str[pos..pos + inlen] == fromstr[fpos..fpos + inlen] {
                matched = true;
                let mut tpos = 0usize;
                let mut tidx = idx;
                while tpos < tostr.len() && tostr[tpos] != 0 {
                    let tolen = char_len(&tostr[tpos..]).min(tostr.len() - tpos);
                    if tidx == 0 {
                        copy_from_to = Some((tpos, tolen));
                        break;
                    }
                    tidx -= 1;
                    tpos += tolen;
                }
                if copy_from_to.is_none() {
                    // tostr is shorter than fromstr.
                    rettv.value = TypvalValue::String(None);
                    return;
                }
                break;
            }
            idx += 1;
            fpos += fromlen;
        }

        if first && !matched {
            first = false;
            let mut tpos = 0usize;
            while tpos < tostr.len() && tostr[tpos] != 0 {
                let tolen = char_len(&tostr[tpos..]).min(tostr.len() - tpos);
                idx -= 1;
                tpos += tolen;
            }
            if idx != 0 {
                rettv.value = TypvalValue::String(None);
                return;
            }
        }

        match copy_from_to {
            Some((tp, tl)) => out.extend_from_slice(&tostr[tp..tp + tl]),
            None => out.extend_from_slice(&in_str[pos..pos + inlen]),
        }

        pos += inlen;
    }

    rettv.value = TypvalValue::String(Some(out));
}

/// `hostname()` - the hostname of the machine Nvim is running on
/// (`f_hostname`, no C implementation - `func_lua = 'f_hostname'` in
/// `eval.lua`, delegating to `runtime/lua/vim/_core/vimfn.lua`'s own
/// `M.f_hostname`, which calls `vim.uv.os_gethostname()`), via the
/// already-existing [`crate::os::env::os_get_hostname`].
fn f_hostname(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::String(Some(crate::os::env::os_get_hostname()));
}

/// `foreground()` - move the Nvim window to the foreground; a no-op
/// in the original itself (empty function body - useful only when
/// sent from a client to a real GUI/terminal server, `f_foreground`,
/// `funcs.c`).
fn f_foreground(_argvars: &[TypvalT], _rettv: &mut TypvalT) {}

/// `eventhandler()` - whether Nvim is currently inside an event
/// handler (`f_eventhandler`, `funcs.c`), via the already-real
/// `GLOBALS.vgetc_busy`.
///
/// # Safety
/// Touches `crate::globals::GLOBALS`.
unsafe fn f_eventhandler(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    let vgetc_busy = unsafe { crate::globals::GLOBALS.get_mut() }.vgetc_busy;
    rettv.value = TypvalValue::Number(i64::from(vgetc_busy));
}

/// `did_filetype()` - whether the `FileType` autocommand event has
/// been triggered at least once for the current buffer
/// (`f_did_filetype`, `funcs.c`), via the already-real
/// `BufT.b_did_filetype`.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
unsafe fn f_did_filetype(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    rettv.value = TypvalValue::Number(i64::from(curbuf.b_did_filetype));
}

/// `garbagecollect([{atexit}])` - request garbage collection at the
/// next opportunity, optionally also at exit (`f_garbagecollect`,
/// `funcs.c`), via the already-real `GLOBALS.want_garbage_collect`/
/// `garbage_collect_at_exit`. The actual collection is postponed to
/// the toplevel by the original itself (it may be running from inside
/// a List/Dict-using expression), matching this crate's own current
/// scope: nothing yet triggers a collection pass in response to this
/// flag, since the toplevel execution loop isn't translated yet - the
/// flag itself is set faithfully regardless.
///
/// # Safety
/// Touches `crate::globals::GLOBALS`.
unsafe fn f_garbagecollect(argvars: &[TypvalT], _rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    globals.want_garbage_collect = true;

    if !argvars.is_empty() && crate::eval::typval::tv_get_number(&argvars[0]) == 1 {
        globals.garbage_collect_at_exit = true;
    }
}

/// `getcharsearch()` - the current character-search
/// (`f`/`F`/`t`/`T`) state as a `Dict` with `"char"`/`"forward"`/
/// `"until"` entries (`f_getcharsearch`, `funcs.c`), via the already-
/// existing [`crate::search::last_csearch_str`]/
/// [`crate::search::last_csearch_forward`]/
/// [`crate::search::last_csearch_until`].
fn f_getcharsearch(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    let d = crate::eval::typval::tv_dict_alloc();
    // SAFETY: `d` was just allocated above and is uniquely owned here.
    let dict = unsafe { &mut *d };
    crate::eval::typval::tv_dict_add_str(dict, b"char", Some(&crate::search::last_csearch_str()));
    crate::eval::typval::tv_dict_add_nr(dict, b"forward", i64::from(crate::search::last_csearch_forward()));
    crate::eval::typval::tv_dict_add_nr(dict, b"until", i64::from(crate::search::last_csearch_until()));
    // SAFETY: `d` is a live, uniquely-owned allocation from tv_dict_alloc above.
    unsafe { crate::eval::typval::tv_dict_set_ret(rettv, d) };
}

/// Whether `argvars[0]` (if present) is a nonzero `Number`/truthy
/// `Bool`/nonempty `String` (`non_zero_arg`, `funcs.c`) - used by
/// `mode()` to decide whether to report the full mode string or just
/// its first character. Missing entirely (`argvars` shorter than 1)
/// is treated the same as the original's own `VAR_UNKNOWN` case:
/// `false`.
fn non_zero_arg(argvars: &[TypvalT]) -> bool {
    match argvars.first().map(|tv| &tv.value) {
        Some(TypvalValue::Number(n)) => *n != 0,
        Some(TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True)) => true,
        Some(TypvalValue::String(Some(s))) => !s.is_empty(),
        _ => false,
    }
}

/// `mode([{expr}])` - a short string describing the current mode
/// (`f_mode`, `funcs.c`), via the already-translated
/// [`crate::state::get_mode`]. Reports only the first character
/// unless `{expr}` is a nonzero `Number`/truthy `Bool`/nonempty
/// `String` (checked via [`non_zero_arg`]).
///
/// # Safety
/// Forwarded from [`crate::state::get_mode`]'s own safety doc.
unsafe fn f_mode(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    let mut buf = unsafe { crate::state::get_mode() };
    if !non_zero_arg(argvars) {
        buf.truncate(1);
    }
    rettv.value = TypvalValue::String(Some(buf));
}

/// `visualmode([{expr}])` - the last Visual mode used in the current
/// buffer (`""`/`"v"`/`"V"`/Ctrl-V), via the already-real
/// `BufT.b_visual_mode_eval` (`f_visualmode`, `funcs.c`). A nonzero
/// `{expr}` (checked via [`non_zero_arg`]) resets it to empty
/// afterward.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
unsafe fn f_visualmode(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    let c = curbuf.b_visual_mode_eval;
    let s = if c == 0 { Vec::new() } else { vec![c as u8] };
    rettv.value = TypvalValue::String(Some(s));

    if non_zero_arg(argvars) {
        curbuf.b_visual_mode_eval = 0;
    }
}

/// `wildmenumode()` - whether the wildmenu is currently active
/// (`f_wildmenumode`, `funcs.c`), via the already-real
/// `GLOBALS.wild_menu_showing`. The original's own second OR operand
/// (`(State & MODE_CMDLINE) && cmdline_pum_active()`, `ex_getln.c`,
/// not translated) is short-circuited away exactly like it is in the
/// original: nothing in this crate can currently set the `MODE_CMDLINE`
/// bit on `State` at all, so it's never actually reached.
///
/// # Safety
/// Touches `crate::globals::GLOBALS`.
#[allow(clippy::diverging_sub_expression)]
unsafe fn f_wildmenumode(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let active = g.wild_menu_showing != 0
        || (g.State & crate::state_defs::mode::CMDLINE as i32 != 0
            && unimplemented!(
                "wildmenumode: MODE_CMDLINE's pum-active state needs ex_getln.c's cmdline_pum_active, not yet translated"
            ));
    if active {
        rettv.value = TypvalValue::Number(1);
    }
    // Matches the original exactly: rettv is left at its DEFAULT
    // value (not explicitly set to 0) when not active - the original
    // never assigns rettv->vval.v_number in that case either, relying
    // on the caller's own zero-initialized rettv.
}

/// `windowsversion()` - the Windows OS version as a String, or empty
/// on non-Windows systems (`f_windowsversion`, `funcs.c`), via the
/// already-real `GLOBALS.windowsVersion` (a fixed `[u8; 20]` buffer,
/// zero-initialized until `main.c`'s own real version-detection code
/// runs - not yet translated, so this is always empty today, matching
/// the original's own real "non-MS-Windows" behavior exactly, not an
/// approximation).
fn f_windowsversion(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    let raw = unsafe { crate::globals::GLOBALS.get_mut() }.windowsVersion;
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    rettv.value = TypvalValue::String(Some(raw[..end].to_vec()));
}

/// The register-name argument shared by `getreg()`/`getregtype()`,
/// defaulting to `v:register` when omitted (`getreg_get_regname`,
/// `funcs.c`). Returns `0` on a type error for an explicit argument;
/// an empty name (explicit or from `v:register`) maps to `'"'` (the
/// unnamed register), matching the original's own
/// `*strregname == 0 ? '"' : ...` fallback.
///
/// # Safety
/// Touches `crate::eval::vars::VIMVARS` (via `get_vim_var_str`).
unsafe fn getreg_get_regname(argvars: &[TypvalT]) -> i32 {
    let strregname = if argvars.is_empty() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::vars::get_vim_var_str(crate::eval::vars::VimVarIndex::Reg) }
    } else {
        match crate::eval::typval::tv_get_string_chk(&argvars[0]) {
            Some(s) => s,
            None => return 0,
        }
    };
    if strregname.is_empty() { i32::from(b'"') } else { i32::from(strregname[0]) }
}

/// `getreg([{regname} [, {expr} [, {list}]]])` - the contents of
/// register `{regname}` (default `v:register`) as a `String`
/// (`f_getreg`, `funcs.c`), via the already-existing
/// [`crate::register::get_reg_contents`]. The `{list}`-truthy path
/// (return a `List` instead) is not yet supported - it panics via
/// `unimplemented!()` inside `get_reg_contents` itself, matching that
/// function's own already-documented deferral (`kGRegList`).
///
/// # Safety
/// Forwarded from [`getreg_get_regname`]/
/// [`crate::register::get_reg_contents`]'s own safety docs.
unsafe fn f_getreg(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    let regname = unsafe { getreg_get_regname(argvars) };
    if regname == 0 {
        return;
    }

    let mut arg2 = false;
    let mut return_list = false;
    if argvars.len() > 1 {
        let mut error = false;
        arg2 = crate::eval::typval::tv_get_number_chk(&argvars[1], Some(&mut error)) != 0;
        if !error && argvars.len() > 2 {
            return_list = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error)) != 0;
        }
        if error {
            return;
        }
    }

    let flags = if arg2 { crate::register_defs::greg_flags::EXPR_SRC } else { 0 };
    if return_list {
        // SAFETY: forwarded from this function's own safety doc. Always
        // panics today (kGRegList not yet supported by get_reg_contents).
        let _ = unsafe { crate::register::get_reg_contents(regname, flags | crate::register_defs::greg_flags::LIST) };
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        let contents = unsafe { crate::register::get_reg_contents(regname, flags) };
        rettv.value = TypvalValue::String(contents);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(n: crate::eval::typval_defs::VarnumberT) -> TypvalT {
        TypvalT { value: TypvalValue::Number(n), ..Default::default() }
    }

    fn string(s: &[u8]) -> TypvalT {
        TypvalT { value: TypvalValue::String(Some(s.to_vec())), ..Default::default() }
    }

    fn float(f: f64) -> TypvalT {
        TypvalT { value: TypvalValue::Float(f), ..Default::default() }
    }

    /// RAII guard temporarily installing a real `GLOBALS.curbuf`, for
    /// functions that transitively read it (e.g. `f_strtrans` ->
    /// [`crate::charset::transstr`] -> `transchar_byte`). Self-locking
    /// (holds `global_state_test_lock()` for its whole lifetime),
    /// matching `charset.rs`'s own established `CurbufGuard` - each
    /// file keeps its own private copy rather than sharing one across
    /// modules, per this crate's established convention.
    struct CurbufGuard {
        previous: *mut crate::buffer_defs::BufT,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CurbufGuard {
        fn set(new_curbuf: *mut crate::buffer_defs::BufT) -> Self {
            let _lock = crate::globals::global_state_test_lock();
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = new_curbuf;
            CurbufGuard { previous, _lock }
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.previous;
        }
    }

    // --- find_internal_func / call_internal_func ---

    #[test]
    fn find_internal_func_finds_a_translated_builtin() {
        assert!(find_internal_func(b"len").is_some());
        assert!(find_internal_func(b"type").is_some());
        assert!(find_internal_func(b"empty").is_some());
    }

    #[test]
    fn find_internal_func_unknown_name_is_none() {
        assert!(find_internal_func(b"not_a_real_function").is_none());
    }

    #[test]
    fn call_internal_func_unknown_name_fails() {
        let mut rettv = TypvalT::default();
        let result = unsafe { call_internal_func(b"not_a_real_function", &[], &mut rettv) };
        assert_eq!(result, FnameTransError::Unknown);
    }

    #[test]
    fn call_internal_func_too_few_arguments_fails() {
        let mut rettv = TypvalT::default();
        let result = unsafe { call_internal_func(b"len", &[], &mut rettv) };
        assert_eq!(result, FnameTransError::TooFew);
    }

    #[test]
    fn call_internal_func_too_many_arguments_fails() {
        let mut rettv = TypvalT::default();
        let args = [num(1), num(2)];
        let result = unsafe { call_internal_func(b"len", &args, &mut rettv) };
        assert_eq!(result, FnameTransError::TooMany);
    }

    #[test]
    fn call_internal_func_dispatches_correctly() {
        let mut rettv = TypvalT::default();
        let args = [string(b"hello")];
        let result = unsafe { call_internal_func(b"len", &args, &mut rettv) };
        assert_eq!(result, FnameTransError::None);
        assert_eq!(rettv.value, TypvalValue::Number(5));
    }

    // --- f_len ---

    #[test]
    fn len_of_a_string() {
        let mut rettv = TypvalT::default();
        unsafe { f_len(&[string(b"hello")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(5));
    }

    #[test]
    fn len_of_a_number_uses_its_string_form() {
        let mut rettv = TypvalT::default();
        unsafe { f_len(&[num(12345)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(5));
    }

    #[test]
    fn len_of_a_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 2);
            crate::eval::typval::tv_list_append_number(&mut *list, 3);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_len(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(3));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn len_of_a_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        let item = crate::eval::typval::tv_dict_item_alloc(b"a");
        unsafe {
            (*item).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *dict, item);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }];
        unsafe { f_len(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn len_of_an_empty_blob() {
        let blob = crate::eval::typval::tv_blob_alloc();
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }];
        unsafe { f_len(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        unsafe { crate::eval::typval::tv_blob_free(blob) };
    }

    // --- f_type ---

    #[test]
    fn type_of_each_scalar_variant() {
        use crate::eval::typval_defs::var_type_result;
        let mut rettv = TypvalT::default();
        f_type(&[num(1)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::NUMBER.into()));
        f_type(&[string(b"s")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::STRING.into()));
        f_type(&[TypvalT { value: TypvalValue::Float(1.0), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::FLOAT.into()));
        f_type(
            &[TypvalT { value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True), ..Default::default() }],
            &mut rettv,
        );
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::BOOL.into()));
        f_type(
            &[TypvalT {
                value: TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null),
                ..Default::default()
            }],
            &mut rettv,
        );
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::SPECIAL.into()));
    }

    #[test]
    fn type_of_list_dict_func_blob() {
        use crate::eval::typval_defs::var_type_result;
        let mut rettv = TypvalT::default();
        f_type(&[TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::LIST.into()));
        f_type(&[TypvalT { value: TypvalValue::Dict(std::ptr::null_mut()), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::DICT.into()));
        f_type(&[TypvalT { value: TypvalValue::Func(None), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::FUNC.into()));
        f_type(&[TypvalT { value: TypvalValue::Partial(std::ptr::null_mut()), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::FUNC.into()));
        f_type(&[TypvalT { value: TypvalValue::Blob(std::ptr::null_mut()), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::BLOB.into()));
    }

    // --- f_empty ---

    #[test]
    fn empty_string_and_number_and_float() {
        let mut rettv = TypvalT::default();
        unsafe { f_empty(&[string(b"")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
        unsafe { f_empty(&[string(b"x")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        unsafe { f_empty(&[num(0)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
        unsafe { f_empty(&[num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn empty_returns_a_number_not_a_bool() {
        // A real, historical quirk: empty() returns a plain Number,
        // not a Vimscript Bool, even though it's conceptually boolean.
        let mut rettv = TypvalT::default();
        unsafe { f_empty(&[num(0)], &mut rettv) };
        assert!(matches!(rettv.value, TypvalValue::Number(_)));
    }

    #[test]
    fn empty_list_and_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        let mut rettv = TypvalT::default();
        unsafe { f_empty(&[TypvalT { value: TypvalValue::List(list), ..Default::default() }], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, 1) };
        unsafe { f_empty(&[TypvalT { value: TypvalValue::List(list), ..Default::default() }], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn empty_null_partial_is_empty() {
        let mut rettv = TypvalT::default();
        unsafe {
            f_empty(&[TypvalT { value: TypvalValue::Partial(std::ptr::null_mut()), ..Default::default() }], &mut rettv);
        }
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn empty_special_null_is_empty() {
        let mut rettv = TypvalT::default();
        unsafe {
            f_empty(
                &[TypvalT {
                    value: TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null),
                    ..Default::default()
                }],
                &mut rettv,
            );
        }
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn empty_bool_true_and_false() {
        let mut rettv = TypvalT::default();
        unsafe {
            f_empty(
                &[TypvalT {
                    value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True),
                    ..Default::default()
                }],
                &mut rettv,
            );
        }
        assert_eq!(rettv.value, TypvalValue::Number(0));
        unsafe {
            f_empty(
                &[TypvalT {
                    value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::False),
                    ..Default::default()
                }],
                &mut rettv,
            );
        }
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    // --- f_and / f_or / f_xor ---

    #[test]
    fn and_of_two_numbers() {
        let mut rettv = TypvalT::default();
        f_and(&[num(0b1100), num(0b1010)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0b1000));
    }

    #[test]
    fn or_of_two_numbers() {
        let mut rettv = TypvalT::default();
        f_or(&[num(0b1100), num(0b1010)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0b1110));
    }

    #[test]
    fn xor_of_two_numbers() {
        let mut rettv = TypvalT::default();
        f_xor(&[num(0b1100), num(0b1010)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0b0110));
    }

    // --- f_abs ---

    #[test]
    fn abs_of_a_positive_and_negative_number() {
        let mut rettv = TypvalT::default();
        f_abs(&[num(5)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(5));
        f_abs(&[num(-5)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(5));
        f_abs(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn abs_of_a_float() {
        let mut rettv = TypvalT::default();
        f_abs(&[TypvalT { value: TypvalValue::Float(-3.5), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(3.5));
    }

    #[test]
    fn abs_of_varnumber_min_wraps_rather_than_panics() {
        let mut rettv = TypvalT::default();
        f_abs(&[num(crate::eval::typval_defs::VARNUMBER_MIN)], &mut rettv);
        // Matches real two's-complement hardware behavior for the
        // original's own unchecked `-n` on this exact input - see
        // f_abs's own doc comment.
        assert_eq!(rettv.value, TypvalValue::Number(crate::eval::typval_defs::VARNUMBER_MIN));
    }

    #[test]
    fn abs_of_a_non_numeric_type_is_negative_one() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let list = crate::eval::typval::tv_list_alloc(0);
        f_abs(&[TypvalT { value: TypvalValue::List(list), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-1));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    // --- max_min / f_max / f_min ---

    #[test]
    fn max_of_a_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 3);
            crate::eval::typval::tv_list_append_number(&mut *list, 7);
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_max(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(7));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn min_of_a_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 3);
            crate::eval::typval::tv_list_append_number(&mut *list, 7);
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_min(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn max_of_an_empty_list_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_max(&args, &mut rettv) };
        // rettv is left untouched (0 is max_min's own pre-set default,
        // not written by this crate - the caller is responsible for
        // it, matching call_func's own "default rettv is number zero"
        // precondition; here it's set explicitly to prove that).
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn max_of_a_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe {
            let item_a = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*item_a).di_tv.value = TypvalValue::Number(2);
            crate::eval::typval::tv_dict_add(&mut *dict, item_a);
            let item_b = crate::eval::typval::tv_dict_item_alloc(b"b");
            (*item_b).di_tv.value = TypvalValue::Number(9);
            crate::eval::typval::tv_dict_add(&mut *dict, item_b);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }];
        unsafe { f_max(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(9));
        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn min_of_an_empty_dict_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }];
        unsafe { f_min(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn max_of_a_non_list_non_dict_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        unsafe { f_max(&[string(b"not a list or dict")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    #[test]
    fn max_of_a_list_with_a_non_numeric_item_leaves_rettv_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        unsafe {
            let inner = crate::eval::typval::tv_list_alloc(0);
            crate::eval::typval::tv_list_append_list(&mut *list, inner);
        }
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_max(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    // --- f_char2nr ---

    #[test]
    fn char2nr_of_ascii_and_multibyte() {
        let mut rettv = TypvalT::default();
        f_char2nr(&[string(b"A")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(i64::from(b'A')));

        f_char2nr(&[string("日".as_bytes())], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0x65E5));
    }

    #[test]
    fn char2nr_of_an_empty_string_is_zero() {
        let mut rettv = TypvalT::default();
        f_char2nr(&[string(b"")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn char2nr_only_uses_the_first_character() {
        let mut rettv = TypvalT::default();
        f_char2nr(&[string(b"BC")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(i64::from(b'B')));
    }

    #[test]
    fn char2nr_accepts_a_valid_second_argument() {
        let mut rettv = TypvalT::default();
        f_char2nr(&[string(b"A"), num(1)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(i64::from(b'A')));
    }

    #[test]
    fn char2nr_rejects_a_non_numeric_second_argument() {
        // tv_check_num's own accepted set includes String (a Vimscript
        // string can coerce to a number) - a genuinely rejected shape
        // needs a List/Dict/Blob/Funcref/Partial instead.
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        f_char2nr(&[string(b"A"), TypvalT { value: TypvalValue::List(list), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    // --- f_nr2char ---

    #[test]
    fn nr2char_of_ascii_and_multibyte() {
        let mut rettv = TypvalT::default();
        f_nr2char(&[num(i64::from(b'A'))], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::String(Some(b"A".to_vec())));

        f_nr2char(&[num(0x65E5)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::String(Some("日".as_bytes().to_vec())));
    }

    #[test]
    fn nr2char_negative_number_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        f_nr2char(&[num(-1)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    #[test]
    fn nr2char_too_large_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        f_nr2char(&[num(i64::from(i32::MAX) + 1)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    // --- f_str2float ---

    #[test]
    fn str2float_parses_a_plain_float() {
        let mut rettv = TypvalT::default();
        f_str2float(&[string(b"12.375")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(12.375));
    }

    #[test]
    fn str2float_handles_a_leading_sign_and_whitespace() {
        let mut rettv = TypvalT::default();
        f_str2float(&[string(b"  -2.5")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(-2.5));

        f_str2float(&[string(b"+ 2.5")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(2.5));
    }

    // --- f_str2nr ---

    #[test]
    fn str2nr_default_base_is_decimal() {
        let mut rettv = TypvalT::default();
        f_str2nr(&[string(b"0123")], &mut rettv);
        // Base 10 explicitly does NOT treat a leading zero as octal.
        assert_eq!(rettv.value, TypvalValue::Number(123));
    }

    #[test]
    fn str2nr_negative_decimal() {
        let mut rettv = TypvalT::default();
        f_str2nr(&[string(b"-42")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-42));
    }

    #[test]
    fn str2nr_hex_base_ignores_leading_0x() {
        let mut rettv = TypvalT::default();
        f_str2nr(&[string(b"0x1A"), num(16)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(26));
    }

    #[test]
    fn str2nr_octal_base_ignores_leading_0o() {
        let mut rettv = TypvalT::default();
        f_str2nr(&[string(b"0o17"), num(8)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(15));
    }

    #[test]
    fn str2nr_binary_base_ignores_leading_0b() {
        let mut rettv = TypvalT::default();
        f_str2nr(&[string(b"0b101"), num(2)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(5));
    }

    #[test]
    fn str2nr_invalid_base_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        f_str2nr(&[string(b"42"), num(3)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    #[test]
    fn str2nr_quoted_ignores_embedded_single_quotes() {
        let mut rettv = TypvalT::default();
        f_str2nr(&[string(b"1'000'000"), num(10), num(1)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1_000_000));
    }

    #[test]
    fn str2nr_without_quoted_flag_stops_at_the_quote() {
        let mut rettv = TypvalT::default();
        f_str2nr(&[string(b"1'000"), num(10)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    // --- f_str2list ---

    #[test]
    fn str2list_of_ascii_and_multibyte() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_str2list(&[string("AB日".as_bytes())], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 3);
        unsafe {
            let mut item = crate::eval::typval::tv_list_first(l);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(i64::from(b'A')));
            item = (*item).li_next;
            assert_eq!((*item).li_tv.value, TypvalValue::Number(i64::from(b'B')));
            item = (*item).li_next;
            assert_eq!((*item).li_tv.value, TypvalValue::Number(0x65E5));
        }
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn str2list_of_an_empty_string_is_an_empty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_str2list(&[string(b"")], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 0);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn str2list_stops_at_an_embedded_nul() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_str2list(&[string(b"AB\0CD")], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 2);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    // --- f_list2str ---

    #[test]
    fn list2str_of_ascii_and_multibyte() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, i64::from(b'A'));
            crate::eval::typval::tv_list_append_number(&mut *list, i64::from(b'B'));
            crate::eval::typval::tv_list_append_number(&mut *list, 0x65E5);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_list2str(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some("AB日".as_bytes().to_vec())));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn list2str_of_a_null_list_is_an_empty_string() {
        let mut rettv = TypvalT::default();
        unsafe { f_list2str(&[TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() }], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    #[test]
    fn list2str_of_a_non_list_is_an_empty_string() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        unsafe { f_list2str(&[string(b"not a list")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    #[test]
    fn str2list_and_list2str_round_trip() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_str2list(&[string("hello 世界".as_bytes())], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };

        let mut rettv2 = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(l), ..Default::default() }];
        unsafe { f_list2str(&args, &mut rettv2) };
        assert_eq!(rettv2.value, TypvalValue::String(Some("hello 世界".as_bytes().to_vec())));

        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    // --- new-builtin table registration ---

    #[test]
    fn new_builtins_are_all_registered() {
        for name in [
            "and",
            "or",
            "xor",
            "abs",
            "max",
            "min",
            "char2nr",
            "nr2char",
            "str2float",
            "str2nr",
            "str2list",
            "list2str",
            "sin",
            "cos",
            "tan",
            "asin",
            "acos",
            "atan",
            "sinh",
            "cosh",
            "tanh",
            "sqrt",
            "exp",
            "log",
            "log10",
            "floor",
            "ceil",
            "round",
            "trunc",
            "atan2",
            "pow",
            "fmod",
            "float2nr",
            "tolower",
            "toupper",
            "trim",
            "has_key",
            "keys",
            "values",
            "items",
            "get",
            "index",
            "reverse",
            "count",
            "copy",
            "deepcopy",
            "add",
            "insert",
            "remove",
            "extend",
            "extendnew",
            "range",
            "repeat",
            "join",
            "flatten",
            "flattennew",
            "localtime",
            "getenv",
            "environ",
            "setenv",
            "has",
            "strlen",
            "strcharlen",
            "strchars",
            "strwidth",
            "stridx",
            "strridx",
            "strgetchar",
            "strpart",
            "strtrans",
            "byteidx",
            "byteidxcomp",
            "charidx",
            "strcharpart",
            "getpid",
            "tr",
            "isdirectory",
            "isabsolutepath",
            "delete",
            "filereadable",
            "filewritable",
            "getfsize",
            "getftime",
            "getftype",
            "pathshorten",
            "hostname",
            "foreground",
            "eventhandler",
            "did_filetype",
            "garbagecollect",
            "getcharsearch",
            "mode",
            "visualmode",
            "wildmenumode",
            "windowsversion",
            "getreg",
            "changenr",
            "interrupt",
            "invert",
            "getfontname",
            "isinf",
            "isnan",
            "sha256",
            "exists",
            "getwinpos",
            "getwinposx",
            "getwinposy",
            "win_getid",
            "win_id2win",
            "win_id2tabwin",
            "win_findbuf",
            "winnr",
        ] {
            assert!(find_internal_func(name.as_bytes()).is_some(), "{name} should be registered");
        }
    }

    // --- single-argument float math builtins ---

    #[test]
    fn sin_cos_tan_of_zero() {
        let mut rettv = TypvalT::default();
        f_sin(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
        f_cos(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(1.0));
        f_tan(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
    }

    #[test]
    fn asin_acos_atan_of_known_values() {
        let mut rettv = TypvalT::default();
        f_asin(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
        f_acos(&[num(1)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
        f_atan(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
    }

    #[test]
    fn sinh_cosh_tanh_of_zero() {
        let mut rettv = TypvalT::default();
        f_sinh(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
        f_cosh(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(1.0));
        f_tanh(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
    }

    #[test]
    fn sqrt_of_a_perfect_square() {
        let mut rettv = TypvalT::default();
        f_sqrt(&[num(100)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(10.0));
    }

    #[test]
    fn sqrt_of_a_negative_number_is_nan() {
        let mut rettv = TypvalT::default();
        f_sqrt(&[float(-4.0)], &mut rettv);
        let TypvalValue::Float(f) = rettv.value else { panic!("expected a Float") };
        assert!(f.is_nan());
    }

    #[test]
    fn exp_of_zero_is_one() {
        let mut rettv = TypvalT::default();
        f_exp(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(1.0));
    }

    #[test]
    fn log_is_the_natural_logarithm() {
        let mut rettv = TypvalT::default();
        f_log(&[float(1.0_f64.exp())], &mut rettv);
        let TypvalValue::Float(f) = rettv.value else { panic!("expected a Float") };
        assert!((f - 1.0).abs() < 1e-9);
    }

    #[test]
    fn log10_of_known_powers_of_ten() {
        // Epsilon comparison, not exact equality: log10 is a
        // libm-delegated transcendental function whose last-bit
        // rounding can legitimately differ slightly between Miri's
        // interpreter and the native platform's own libm (confirmed:
        // Miri gives log10(1000) as 2.9999999999999996, not exactly
        // 3.0) - this is an execution-environment precision quirk, not
        // a logic bug, so the test itself should tolerate it rather
        // than assert bit-exact equality.
        fn assert_close(actual: &TypvalValue, expected: f64) {
            let TypvalValue::Float(f) = *actual else { panic!("expected a Float") };
            assert!((f - expected).abs() < 1e-9, "{f} not close to {expected}");
        }

        let mut rettv = TypvalT::default();
        f_log10(&[num(1000)], &mut rettv);
        assert_close(&rettv.value, 3.0);
        f_log10(&[float(0.01)], &mut rettv);
        assert_close(&rettv.value, -2.0);
    }

    #[test]
    fn floor_ceil_round_trunc_of_a_fraction() {
        let mut rettv = TypvalT::default();
        f_floor(&[float(1.856)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(1.0));
        f_ceil(&[float(1.856)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(2.0));
        f_round(&[float(4.5)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(5.0));
        f_trunc(&[float(1.856)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(1.0));
    }

    #[test]
    fn floor_ceil_round_trunc_of_negative_numbers() {
        // Matches the original's own documented examples exactly:
        // ceil(-5.456) == -5.0, round(-4.5) == -5.0 (away from zero).
        let mut rettv = TypvalT::default();
        f_ceil(&[float(-5.456)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(-5.0));
        f_floor(&[float(-5.456)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(-6.0));
        f_round(&[float(-4.5)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(-5.0));
        f_trunc(&[float(-5.456)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(-5.0));
    }

    #[test]
    fn float_op_wrapper_functions_return_zero_for_non_numeric_input() {
        let mut rettv = TypvalT::default();
        f_sin(&[string(b"not a number")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
    }

    // --- two-argument float math builtins ---

    #[test]
    fn atan2_of_known_values() {
        let mut rettv = TypvalT::default();
        f_atan2(&[num(0), num(1)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
    }

    #[test]
    fn pow_of_known_values() {
        // Epsilon comparison, not exact equality - see
        // log10_of_known_powers_of_ten's own comment: pow(2, 10) is
        // 1024.0000000000002 under Miri's own libm, not exactly
        // 1024.0, a real (if tiny) execution-environment precision
        // difference for this transcendental function, not a bug.
        let mut rettv = TypvalT::default();
        f_pow(&[num(2), num(10)], &mut rettv);
        let TypvalValue::Float(f) = rettv.value else { panic!("expected a Float") };
        assert!((f - 1024.0).abs() < 1e-9, "{f} not close to 1024.0");
    }

    #[test]
    fn fmod_matches_truncating_c_style_remainder_not_ieee_remainder() {
        // fmod(-5.0, 3.0) == -2.0 (truncating division, sign matches
        // the dividend) - this is the ONE input shape that would
        // distinguish fmod from f64::rem_euclid (which would give 1.0)
        // and from the IEEE round-to-even `remainder()` operation
        // (which would also give 1.0 here) - proves this crate's `%`
        // really does match C's fmod, not a different remainder op.
        let mut rettv = TypvalT::default();
        f_fmod(&[float(-5.0), num(3)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(-2.0));
        f_fmod(&[float(5.0), num(3)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(2.0));
    }

    #[test]
    fn two_arg_float_functions_return_zero_when_either_argument_is_non_numeric() {
        let mut rettv = TypvalT::default();
        f_pow(&[num(2), string(b"x")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
        f_pow(&[string(b"x"), num(2)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
    }

    // --- f_float2nr ---

    #[test]
    fn float2nr_truncates_toward_zero() {
        let mut rettv = TypvalT::default();
        f_float2nr(&[float(3.9)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(3));
        f_float2nr(&[float(-3.9)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-3));
    }

    #[test]
    fn float2nr_clamps_a_huge_positive_float_to_varnumber_max() {
        let mut rettv = TypvalT::default();
        f_float2nr(&[float(1e300)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(crate::eval::typval_defs::VARNUMBER_MAX));
    }

    #[test]
    fn float2nr_clamps_a_huge_negative_float_to_negative_varnumber_max() {
        let mut rettv = TypvalT::default();
        f_float2nr(&[float(-1e300)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-crate::eval::typval_defs::VARNUMBER_MAX));
    }

    #[test]
    fn float2nr_non_numeric_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        f_float2nr(&[string(b"not a number")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    // --- f_tolower / f_toupper ---

    #[test]
    fn tolower_converts_ascii_and_multibyte() {
        let mut rettv = TypvalT::default();
        unsafe { f_tolower(&[string(b"Hello WORLD")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hello world".to_vec())));

        unsafe { f_tolower(&[string("HÉLLO".as_bytes())], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some("héllo".as_bytes().to_vec())));
    }

    #[test]
    fn toupper_converts_ascii_and_multibyte() {
        let mut rettv = TypvalT::default();
        unsafe { f_toupper(&[string(b"Hello world")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"HELLO WORLD".to_vec())));

        unsafe { f_toupper(&[string("héllo".as_bytes())], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some("HÉLLO".as_bytes().to_vec())));
    }

    #[test]
    fn tolower_toupper_of_an_empty_string() {
        let mut rettv = TypvalT::default();
        unsafe { f_tolower(&[string(b"")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(Vec::new())));
        unsafe { f_toupper(&[string(b"")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(Vec::new())));
    }

    // --- f_trim ---

    #[test]
    fn trim_default_mask_trims_both_ends() {
        let mut rettv = TypvalT::default();
        unsafe { f_trim(&[string(b"  hello world  ")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hello world".to_vec())));
    }

    #[test]
    fn trim_default_mask_trims_non_breaking_space_too() {
        let mut rettv = TypvalT::default();
        unsafe { f_trim(&[string("\u{a0}hi\u{a0}".as_bytes())], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hi".to_vec())));
    }

    #[test]
    fn trim_leading_only() {
        let mut rettv = TypvalT::default();
        unsafe { f_trim(&[string(b"  hello  "), string(b" "), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hello  ".to_vec())));
    }

    #[test]
    fn trim_trailing_only() {
        let mut rettv = TypvalT::default();
        unsafe { f_trim(&[string(b"  hello  "), string(b" "), num(2)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"  hello".to_vec())));
    }

    #[test]
    fn trim_custom_mask() {
        let mut rettv = TypvalT::default();
        unsafe { f_trim(&[string(b"xxhelloxx"), string(b"x")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hello".to_vec())));
    }

    #[test]
    fn trim_empty_mask_falls_back_to_default() {
        let mut rettv = TypvalT::default();
        unsafe { f_trim(&[string(b"  hi  "), string(b"")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hi".to_vec())));
    }

    #[test]
    fn trim_invalid_dir_resets_rettv_to_an_empty_string() {
        // Unlike most other builtins here (which leave rettv at its
        // caller-provided default on error), f_trim's own C original
        // unconditionally resets rettv to an empty String BEFORE any
        // argument validation (`rettv->v_type = VAR_STRING; rettv->
        // vval.v_string = NULL;` at the very top) - so an invalid dir
        // still leaves rettv String(None), not the pre-call value.
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        unsafe { f_trim(&[string(b"hi"), string(b" "), num(3)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    #[test]
    fn trim_dir_is_ignored_when_mask_argument_is_absent() {
        // dir is only ever consulted when mask was ACTUALLY passed as a
        // real String - this can't literally be exercised with a 3rd
        // positional arg without a 2nd (max_argc requires args in
        // order), so this documents the invariant via the table's own
        // arity instead: trim() cannot be called with a dir but no
        // mask at all, matching the original's own argument order.
        assert_eq!(find_internal_func(b"trim").unwrap().min_argc, 1);
    }

    #[test]
    fn trim_multibyte_text_and_mask() {
        let mut rettv = TypvalT::default();
        unsafe { f_trim(&[string("日日hello日日".as_bytes()), string("日".as_bytes())], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hello".to_vec())));
    }

    // --- f_has_key ---

    #[test]
    fn has_key_finds_a_present_key_and_rejects_a_missing_one() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe {
            let item = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*item).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *dict, item);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }, string(b"a")];
        f_has_key(&args, &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));

        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }, string(b"nope")];
        f_has_key(&args, &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));

        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn has_key_null_dict_is_false() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [
            TypvalT { value: TypvalValue::Dict(std::ptr::null_mut()), ..Default::default() },
            string(b"a"),
        ];
        f_has_key(&args, &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    // --- f_keys / f_values / f_items ---

    fn make_test_dict() -> *mut crate::eval::typval_defs::DictT {
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe {
            let item_a = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*item_a).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *dict, item_a);
            let item_b = crate::eval::typval::tv_dict_item_alloc(b"b");
            (*item_b).di_tv.value = TypvalValue::Number(2);
            crate::eval::typval::tv_dict_add(&mut *dict, item_b);
        }
        dict
    }

    #[test]
    fn keys_returns_a_list_of_key_strings() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = make_test_dict();
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }];
        unsafe { f_keys(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 2);
        let mut keys: Vec<Vec<u8>> = Vec::new();
        unsafe {
            let mut item = crate::eval::typval::tv_list_first(l);
            while !item.is_null() {
                let TypvalValue::String(Some(s)) = &(*item).li_tv.value else { panic!("expected a String") };
                keys.push(s.clone());
                item = (*item).li_next;
            }
        }
        keys.sort();
        assert_eq!(keys, vec![b"a".to_vec(), b"b".to_vec()]);
        unsafe {
            crate::eval::typval::tv_list_unref(l);
            crate::eval::typval::tv_dict_unref(dict);
        }
    }

    #[test]
    fn values_returns_a_list_of_the_dicts_own_values() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = make_test_dict();
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }];
        unsafe { f_values(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 2);
        let mut values: Vec<crate::eval::typval_defs::VarnumberT> = Vec::new();
        unsafe {
            let mut item = crate::eval::typval::tv_list_first(l);
            while !item.is_null() {
                let TypvalValue::Number(n) = (*item).li_tv.value else { panic!("expected a Number") };
                values.push(n);
                item = (*item).li_next;
            }
        }
        values.sort_unstable();
        assert_eq!(values, vec![1, 2]);
        unsafe {
            crate::eval::typval::tv_list_unref(l);
            crate::eval::typval::tv_dict_unref(dict);
        }
    }

    #[test]
    fn items_returns_a_list_of_key_value_pairs() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe {
            let item_a = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*item_a).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *dict, item_a);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }];
        unsafe { f_items(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 1);
        unsafe {
            let pair_item = crate::eval::typval::tv_list_first(l);
            let TypvalValue::List(pair) = (*pair_item).li_tv.value else { panic!("expected a List") };
            assert_eq!(crate::eval::typval::tv_list_len(pair), 2);
            let key_item = crate::eval::typval::tv_list_first(pair);
            assert_eq!((*key_item).li_tv.value, TypvalValue::String(Some(b"a".to_vec())));
            let value_item = (*key_item).li_next;
            assert_eq!((*value_item).li_tv.value, TypvalValue::Number(1));

            crate::eval::typval::tv_list_unref(l);
            crate::eval::typval::tv_dict_unref(dict);
        }
    }

    #[test]
    fn keys_values_items_of_a_null_dict_are_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let args = [TypvalT { value: TypvalValue::Dict(std::ptr::null_mut()), ..Default::default() }];
        for f in [f_keys, f_values] {
            let mut rettv = TypvalT::default();
            unsafe { f(&args, &mut rettv) };
            let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
            assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 0);
            unsafe { crate::eval::typval::tv_list_unref(l) };
        }
    }

    #[test]
    fn keys_of_a_non_dict_is_an_empty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_keys(&[num(5)], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 0);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn items_of_a_list_argument_is_unimplemented() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        let mut rettv = TypvalT::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            f_items(&args, &mut rettv);
        }));
        assert!(result.is_err(), "expected a panic (tv_list2items not yet translated)");
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    // --- f_get ---

    #[test]
    fn get_from_a_list_in_range_and_out_of_range() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 10);
            crate::eval::typval::tv_list_append_number(&mut *list, 20);
            crate::eval::typval::tv_list_append_number(&mut *list, 30);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(1)];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(20));

        // Negative index counts from the end.
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(-1)];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(30));

        // Out of range with a default.
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(99), string(b"missing")];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"missing".to_vec())));

        // Out of range without a default.
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(99)];
        rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));

        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn get_from_a_null_list_uses_the_default() {
        let mut rettv = TypvalT::default();
        let args = [
            TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() },
            num(0),
            num(42),
        ];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(42));
    }

    #[test]
    fn get_from_a_dict_present_and_missing_key() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe {
            let item = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*item).di_tv.value = TypvalValue::Number(7);
            crate::eval::typval::tv_dict_add(&mut *dict, item);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }, string(b"a")];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(7));

        let args =
            [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }, string(b"missing"), num(-1)];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));

        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn get_from_a_blob_in_range_and_out_of_range() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![10, 20, 30];
            (*blob).bv_ga.ga_len = 3;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(1)];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(20));

        // Out of range with no default -> -1 (blob's own sentinel).
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(99)];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));

        // Out of range WITH a default -> the default overrides the -1
        // sentinel, matching the original's own shared tail exactly.
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(99), num(77)];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(77));

        unsafe { crate::eval::typval::tv_blob_free(blob) };
    }

    #[test]
    fn get_of_a_non_container_falls_back_to_the_default() {
        let mut rettv = TypvalT::default();
        let args = [num(5), num(0), string(b"fallback")];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"fallback".to_vec())));
    }

    #[test]
    fn get_of_a_funcref_is_unimplemented() {
        let args = [TypvalT { value: TypvalValue::Func(Some(b"len".to_vec())), ..Default::default() }, string(b"name")];
        let mut rettv = TypvalT::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            f_get(&args, &mut rettv);
        }));
        assert!(result.is_err(), "expected a panic (get_func_arity/partial introspection not yet translated)");
    }

    // --- f_index ---

    #[test]
    fn index_finds_a_present_number_in_a_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 10);
            crate::eval::typval::tv_list_append_number(&mut *list, 20);
            crate::eval::typval::tv_list_append_number(&mut *list, 30);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(20)];
        unsafe { f_index(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn index_of_a_missing_value_is_negative_one() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, 1) };
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(99)];
        unsafe { f_index(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn index_with_a_start_position() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(4);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 5);
            crate::eval::typval::tv_list_append_number(&mut *list, 5);
            crate::eval::typval::tv_list_append_number(&mut *list, 5);
            crate::eval::typval::tv_list_append_number(&mut *list, 5);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(5), num(2)];
        unsafe { f_index(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(2));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn index_case_insensitive_string_match() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_append_string(list, Some(b"Hello")) };
        let mut rettv = TypvalT::default();
        // Case-sensitive: no match.
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, string(b"hello"), num(0)];
        unsafe { f_index(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));
        // Case-insensitive: matches.
        let args =
            [TypvalT { value: TypvalValue::List(list), ..Default::default() }, string(b"hello"), num(0), num(1)];
        unsafe { f_index(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn index_in_a_blob() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![10, 20, 30];
            (*blob).bv_ga.ga_len = 3;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(20)];
        unsafe { f_index(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));

        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(99)];
        unsafe { f_index(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));

        unsafe { crate::eval::typval::tv_blob_free(blob) };
    }

    #[test]
    fn index_of_a_non_list_non_blob_is_negative_one() {
        let mut rettv = TypvalT::default();
        unsafe { f_index(&[string(b"not a list"), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    #[test]
    fn index_of_a_null_list_is_negative_one() {
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() }, num(1)];
        unsafe { f_index(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    // --- f_reverse / reverse_text ---

    #[test]
    fn reverse_text_reverses_ascii() {
        let result = unsafe { reverse_text(b"hello") };
        assert_eq!(result, b"olleh");
    }

    #[test]
    fn reverse_text_keeps_multibyte_characters_intact() {
        let result = unsafe { reverse_text("ab日本cd".as_bytes()) };
        assert_eq!(result, "dc本日ba".as_bytes());
    }

    #[test]
    fn reverse_text_stops_at_an_embedded_nul() {
        let result = unsafe { reverse_text(b"ab\0cd") };
        assert_eq!(result, b"ba");
    }

    #[test]
    fn reverse_of_a_string() {
        let mut rettv = TypvalT::default();
        unsafe { f_reverse(&[string(b"hello")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"olleh".to_vec())));
    }

    #[test]
    fn reverse_of_a_null_string() {
        let mut rettv = TypvalT::default();
        unsafe { f_reverse(&[TypvalT { value: TypvalValue::String(None), ..Default::default() }], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    #[test]
    fn reverse_of_a_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 2);
            crate::eval::typval::tv_list_append_number(&mut *list, 3);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_reverse(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(l, list); // reversed in place, same pointer returned.
        unsafe {
            let mut item = crate::eval::typval::tv_list_first(l);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(3));
            item = (*item).li_next;
            assert_eq!((*item).li_tv.value, TypvalValue::Number(2));
            item = (*item).li_next;
            assert_eq!((*item).li_tv.value, TypvalValue::Number(1));
            crate::eval::typval::tv_list_unref(l);
        }
    }

    #[test]
    fn reverse_of_a_locked_list_leaves_it_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 2);
            (*list).lv_lock = crate::eval::typval_defs::VarLockStatus::Locked;
        }
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_reverse(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe {
            let item = crate::eval::typval::tv_list_first(list);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(1)); // unchanged order.
            crate::eval::typval::tv_list_unref(list);
        }
    }

    #[test]
    fn reverse_of_a_blob() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![1, 2, 3, 4];
            (*blob).bv_ga.ga_len = 4;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }];
        unsafe { f_reverse(&args, &mut rettv) };
        let TypvalValue::Blob(b) = rettv.value else { panic!("expected a Blob") };
        assert_eq!(b, blob);
        unsafe {
            assert_eq!((*b).bv_ga.ga_data, vec![4, 3, 2, 1]);
            crate::eval::typval::tv_blob_free(blob);
        }
    }

    #[test]
    fn reverse_of_an_odd_length_blob() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![1, 2, 3];
            (*blob).bv_ga.ga_len = 3;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }];
        unsafe { f_reverse(&args, &mut rettv) };
        unsafe {
            assert_eq!((*blob).bv_ga.ga_data, vec![3, 2, 1]);
            crate::eval::typval::tv_blob_free(blob);
        }
    }

    #[test]
    fn reverse_of_a_number_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        unsafe { f_reverse(&[num(5)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    // --- f_count ---

    #[test]
    fn count_in_a_string_case_sensitive_and_insensitive() {
        let mut rettv = TypvalT::default();
        unsafe { f_count(&[string(b"ababab"), string(b"ab")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(3));

        unsafe { f_count(&[string(b"ABabAB"), string(b"ab")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));

        unsafe { f_count(&[string(b"ABabAB"), string(b"ab"), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(3));
    }

    #[test]
    fn count_in_a_string_with_an_empty_needle_is_zero() {
        let mut rettv = TypvalT::default();
        unsafe { f_count(&[string(b"hello"), string(b"")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn count_in_a_string_stops_at_an_embedded_nul() {
        let mut rettv = TypvalT::default();
        unsafe { f_count(&[string(b"ab\0abab"), string(b"ab")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn count_in_a_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(4);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 2);
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(1)];
        unsafe { f_count(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(3));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn count_in_a_list_starting_at_an_index() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(4);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
        }
        let mut rettv = TypvalT::default();
        // ic=0, start=2 - both must be given for start to take effect.
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(1), num(0), num(2)];
        unsafe { f_count(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(2));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn count_in_a_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe {
            let item_a = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*item_a).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *dict, item_a);
            let item_b = crate::eval::typval::tv_dict_item_alloc(b"b");
            (*item_b).di_tv.value = TypvalValue::Number(2);
            crate::eval::typval::tv_dict_add(&mut *dict, item_b);
            let item_c = crate::eval::typval::tv_dict_item_alloc(b"c");
            (*item_c).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *dict, item_c);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }, num(1)];
        unsafe { f_count(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(2));
        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn count_in_a_dict_with_both_ic_and_start_is_an_error() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe {
            let item = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*item).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *dict, item);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }, num(1), num(0), num(0)];
        unsafe { f_count(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn count_of_a_null_dict_is_zero() {
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(std::ptr::null_mut()), ..Default::default() }, num(1)];
        unsafe { f_count(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn count_of_a_non_container_is_zero() {
        let mut rettv = TypvalT::default();
        unsafe { f_count(&[num(5), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    // --- f_copy ---

    #[test]
    fn copy_of_a_list_is_a_genuinely_separate_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 2);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_copy(&args, &mut rettv) };
        let TypvalValue::List(copy) = rettv.value else { panic!("expected a List") };
        assert_ne!(copy, list); // a genuinely separate list, not an alias.
        assert_eq!(rettv.v_lock, crate::eval::typval_defs::VarLockStatus::Unlocked);
        unsafe {
            assert_eq!((*copy).lv_refcount, 1); // set exactly once, not double-counted.
            assert_eq!((*list).lv_refcount, 0); // the original's own refcount is untouched.

            // Mutating the copy must not affect the original.
            let item = crate::eval::typval::tv_list_first(copy);
            (*item).li_tv.value = TypvalValue::Number(99);
            let orig_item = crate::eval::typval::tv_list_first(list);
            assert_eq!((*orig_item).li_tv.value, TypvalValue::Number(1));

            crate::eval::typval::tv_list_unref(copy);
            crate::eval::typval::tv_list_unref(list);
        }
    }

    #[test]
    fn copy_of_a_null_list_is_a_null_list() {
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() }];
        unsafe { f_copy(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::List(std::ptr::null_mut()));
    }

    #[test]
    fn copy_of_a_dict_is_a_genuinely_separate_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = make_test_dict();
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }];
        unsafe { f_copy(&args, &mut rettv) };
        let TypvalValue::Dict(copy) = rettv.value else { panic!("expected a Dict") };
        assert_ne!(copy, dict); // a genuinely separate dict, not an alias.
        assert_eq!(rettv.v_lock, crate::eval::typval_defs::VarLockStatus::Unlocked);
        unsafe {
            assert_eq!((*copy).dv_refcount, 1); // set exactly once, not double-counted.
            assert_eq!((*dict).dv_refcount, 0); // the original's own refcount is untouched.
            assert_eq!(crate::eval::typval::tv_dict_len(copy.as_ref()), 2);

            // Mutating the copy must not affect the original.
            let item = crate::eval::typval::tv_dict_find(Some(&mut *copy), b"a").unwrap();
            (*item).di_tv.value = TypvalValue::Number(99);
            let orig_item = crate::eval::typval::tv_dict_find(Some(&mut *dict), b"a").unwrap();
            assert_eq!((*orig_item).di_tv.value, TypvalValue::Number(1));

            crate::eval::typval::tv_dict_unref(copy);
            crate::eval::typval::tv_dict_unref(dict);
        }
    }

    #[test]
    fn copy_of_a_null_dict_is_a_null_dict() {
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(std::ptr::null_mut()), ..Default::default() }];
        unsafe { f_copy(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Dict(std::ptr::null_mut()));
    }

    #[test]
    fn copy_of_a_blob_is_a_genuinely_separate_blob() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![1, 2, 3];
            (*blob).bv_ga.ga_len = 3;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }];
        unsafe { f_copy(&args, &mut rettv) };
        let TypvalValue::Blob(copy) = rettv.value else { panic!("expected a Blob") };
        assert_ne!(copy, blob); // a genuinely separate blob, not an alias.
        unsafe {
            assert_eq!((*copy).bv_ga.ga_data, vec![1, 2, 3]);
            assert_eq!((*copy).bv_refcount, 1);
            assert_eq!((*blob).bv_refcount, 0);

            // Mutating the copy must not affect the original.
            (&mut (*copy).bv_ga.ga_data)[0] = 99;
            assert_eq!((&(*blob).bv_ga.ga_data)[0], 1);

            crate::eval::typval::tv_blob_free(copy);
            crate::eval::typval::tv_blob_free(blob);
        }
    }

    #[test]
    fn copy_of_a_number_is_a_plain_value_copy() {
        let mut rettv = TypvalT::default();
        unsafe { f_copy(&[num(42)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(42));
    }

    #[test]
    fn copy_of_a_string_is_a_plain_value_copy() {
        let mut rettv = TypvalT::default();
        unsafe { f_copy(&[string(b"hello")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hello".to_vec())));
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "f_copy(UNKNOWN)")]
    fn copy_of_unknown_panics_in_debug() {
        let mut rettv = TypvalT::default();
        unsafe { f_copy(&[TypvalT::default()], &mut rettv) };
    }

    // --- f_deepcopy ---

    #[test]
    fn deepcopy_of_a_nested_list_copies_recursively() {
        let _lock = crate::globals::global_state_test_lock();
        let inner = crate::eval::typval::tv_list_alloc(1);
        unsafe {
            crate::eval::typval::tv_list_ref(inner);
            crate::eval::typval::tv_list_append_number(&mut *inner, 1);
        }
        let outer = crate::eval::typval::tv_list_alloc(1);
        unsafe {
            crate::eval::typval::tv_list_append_owned_tv(
                outer,
                TypvalT { value: TypvalValue::List(inner), ..Default::default() },
            )
        };
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(outer), ..Default::default() }];
        unsafe { f_deepcopy(&args, &mut rettv) };
        let TypvalValue::List(outer_copy) = rettv.value else { panic!("expected a List") };
        assert_ne!(outer_copy, outer);
        unsafe {
            let item = crate::eval::typval::tv_list_first(outer_copy);
            let TypvalValue::List(inner_copy) = (*item).li_tv.value else { panic!("expected a List") };
            // Deep copy: the nested list is ALSO a genuinely separate
            // copy, not the same pointer as the original's own inner
            // list (contrast copy()'s own shallow behavior).
            assert_ne!(inner_copy, inner);

            // Mutating the nested copy must not affect the nested
            // original.
            let inner_copy_item = crate::eval::typval::tv_list_first(inner_copy);
            (*inner_copy_item).li_tv.value = TypvalValue::Number(99);
            let inner_orig_item = crate::eval::typval::tv_list_first(inner);
            assert_eq!((*inner_orig_item).li_tv.value, TypvalValue::Number(1));

            crate::eval::typval::tv_list_unref(outer);
            crate::eval::typval::tv_list_unref(outer_copy);
        }
    }

    #[test]
    fn deepcopy_of_a_nested_dict_copies_recursively() {
        let _lock = crate::globals::global_state_test_lock();
        let inner = crate::eval::typval::tv_dict_alloc();
        unsafe {
            (*inner).dv_refcount += 1;
            let item = crate::eval::typval::tv_dict_item_alloc(b"x");
            (*item).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *inner, item);
        }
        let outer = crate::eval::typval::tv_dict_alloc();
        unsafe {
            let item = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*item).di_tv.value = TypvalValue::Dict(inner);
            crate::eval::typval::tv_dict_add(&mut *outer, item);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(outer), ..Default::default() }];
        unsafe { f_deepcopy(&args, &mut rettv) };
        let TypvalValue::Dict(outer_copy) = rettv.value else { panic!("expected a Dict") };
        assert_ne!(outer_copy, outer);
        unsafe {
            let item = crate::eval::typval::tv_dict_find(Some(&mut *outer_copy), b"a").unwrap();
            let TypvalValue::Dict(inner_copy) = (*item).di_tv.value else { panic!("expected a Dict") };
            assert_ne!(inner_copy, inner);

            let inner_copy_item = crate::eval::typval::tv_dict_find(Some(&mut *inner_copy), b"x").unwrap();
            (*inner_copy_item).di_tv.value = TypvalValue::Number(99);
            let inner_orig_item = crate::eval::typval::tv_dict_find(Some(&mut *inner), b"x").unwrap();
            assert_eq!((*inner_orig_item).di_tv.value, TypvalValue::Number(1));

            crate::eval::typval::tv_dict_unref(outer);
            crate::eval::typval::tv_dict_unref(outer_copy);
        }
    }

    #[test]
    fn deepcopy_without_noref_reuses_the_same_copy_for_a_shared_reference() {
        // deepcopy(x) (noref omitted, defaults to 0): the SAME list
        // referenced twice produces the SAME copy both times.
        let _lock = crate::globals::global_state_test_lock();
        let inner = crate::eval::typval::tv_list_alloc(0);
        unsafe { crate::eval::typval::tv_list_ref(inner) };
        let outer = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_owned_tv(
                outer,
                TypvalT { value: TypvalValue::List(inner), ..Default::default() },
            );
            crate::eval::typval::tv_list_ref(inner);
            crate::eval::typval::tv_list_append_owned_tv(
                outer,
                TypvalT { value: TypvalValue::List(inner), ..Default::default() },
            );
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(outer), ..Default::default() }];
        unsafe { f_deepcopy(&args, &mut rettv) };
        let TypvalValue::List(outer_copy) = rettv.value else { panic!("expected a List") };
        unsafe {
            let first = crate::eval::typval::tv_list_first(outer_copy);
            let second = (*first).li_next;
            let TypvalValue::List(first_copy) = (*first).li_tv.value else { panic!("expected a List") };
            let TypvalValue::List(second_copy) = (*second).li_tv.value else { panic!("expected a List") };
            assert_eq!(first_copy, second_copy);

            crate::eval::typval::tv_list_unref(outer);
            crate::eval::typval::tv_list_unref(outer_copy);
        }
    }

    #[test]
    fn deepcopy_with_noref_1_makes_separate_copies_for_a_shared_reference() {
        // deepcopy(x, 1): every occurrence of the same referenced
        // list gets its OWN separate copy instead.
        let _lock = crate::globals::global_state_test_lock();
        let inner = crate::eval::typval::tv_list_alloc(0);
        unsafe { crate::eval::typval::tv_list_ref(inner) };
        let outer = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_owned_tv(
                outer,
                TypvalT { value: TypvalValue::List(inner), ..Default::default() },
            );
            crate::eval::typval::tv_list_ref(inner);
            crate::eval::typval::tv_list_append_owned_tv(
                outer,
                TypvalT { value: TypvalValue::List(inner), ..Default::default() },
            );
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(outer), ..Default::default() }, num(1)];
        unsafe { f_deepcopy(&args, &mut rettv) };
        let TypvalValue::List(outer_copy) = rettv.value else { panic!("expected a List") };
        unsafe {
            let first = crate::eval::typval::tv_list_first(outer_copy);
            let second = (*first).li_next;
            let TypvalValue::List(first_copy) = (*first).li_tv.value else { panic!("expected a List") };
            let TypvalValue::List(second_copy) = (*second).li_tv.value else { panic!("expected a List") };
            assert_ne!(first_copy, second_copy);

            crate::eval::typval::tv_list_unref(outer);
            crate::eval::typval::tv_list_unref(outer_copy);
        }
    }

    #[test]
    fn deepcopy_of_a_number_is_a_plain_value_copy() {
        let mut rettv = TypvalT::default();
        unsafe { f_deepcopy(&[num(42)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(42));
    }

    #[test]
    fn deepcopy_with_only_the_required_argument_does_not_panic() {
        // argvars.len() == 1 - the optional noref argument is simply
        // absent, not an in-bounds Unknown-typed sentinel like the
        // original's own fixed-size argvars array would have.
        let mut rettv = TypvalT::default();
        unsafe { f_deepcopy(&[num(7)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(7));
    }

    // --- f_add ---

    #[test]
    fn add_appends_to_a_list_and_returns_the_same_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        // Not ref'd upfront - f_add's own successful-path tv_copy(&argvars[0],
        // rettv) is the ONE reference this test needs to release at
        // the end, matching f_reverse's own already-established test
        // pattern (tv_list_set_ret/tv_copy both increment refcount
        // exactly once on their own success path).
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, 1) };
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(2)];
        unsafe { f_add(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(l, list); // same list, mutated in place.
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 2);
            let item = crate::eval::typval::tv_list_first(l);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(1));
            let item2 = (*item).li_next;
            assert_eq!((*item2).li_tv.value, TypvalValue::Number(2));
            crate::eval::typval::tv_list_unref(list);
        }
    }

    #[test]
    fn add_appends_a_list_as_a_single_nested_item_not_concatenated() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, 1) };
        // `inner` is not ref'd upfront either - f_add's own
        // tv_list_append_tv(l, &argvars[1]) internally tv_copy's
        // argvars[1] (List(inner)), incrementing inner's refcount
        // exactly once; unreffing `list` at the end (which then frees
        // it, given `list` itself is also only ref'd via f_add's own
        // rettv copy) cascades into releasing that same reference via
        // tv_list_free_contents, so inner needs no separate unref here.
        let inner = crate::eval::typval::tv_list_alloc(0);
        let mut rettv = TypvalT::default();
        let args = [
            TypvalT { value: TypvalValue::List(list), ..Default::default() },
            TypvalT { value: TypvalValue::List(inner), ..Default::default() },
        ];
        unsafe { f_add(&args, &mut rettv) };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(list), 2);
            let item = crate::eval::typval::tv_list_first(list);
            let item2 = (*item).li_next;
            let TypvalValue::List(nested) = (*item2).li_tv.value else { panic!("expected a nested List") };
            assert_eq!(nested, inner); // appended by reference, not flattened.
            crate::eval::typval::tv_list_unref(list);
        }
    }

    #[test]
    fn add_to_a_locked_list_leaves_it_untouched_and_returns_1() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        unsafe {
            crate::eval::typval::tv_list_ref(list);
            (*list).lv_lock = crate::eval::typval_defs::VarLockStatus::Locked;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(1)];
        unsafe { f_add(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1)); // failed.
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(list), 0);
            crate::eval::typval::tv_list_unref(list);
        }
    }

    #[test]
    fn add_appends_a_byte_to_a_blob_and_returns_the_same_blob() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_refcount += 1;
            (*blob).bv_ga.ga_data = vec![1, 2];
            (*blob).bv_ga.ga_len = 2;
            (*blob).bv_ga.ga_maxlen = 2;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(3)];
        unsafe { f_add(&args, &mut rettv) };
        let TypvalValue::Blob(b) = rettv.value else { panic!("expected a Blob") };
        assert_eq!(b, blob);
        unsafe {
            // ga_grow may over-allocate ga_data's own capacity beyond
            // ga_len (matching the original growarray's own amortized-
            // growth strategy) - tv_blob_len/tv_blob_get, not a raw
            // ga_data comparison, are the correct way to inspect a
            // blob's own LOGICAL contents.
            assert_eq!(crate::eval::typval::tv_blob_len(b), 3);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 0), 1);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 1), 2);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 2), 3);
            crate::eval::typval::tv_blob_free(blob);
        }
    }

    #[test]
    fn add_to_a_null_blob_returns_1() {
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(std::ptr::null_mut()), ..Default::default() }, num(1)];
        unsafe { f_add(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn add_to_a_non_list_non_blob_returns_1() {
        let mut rettv = TypvalT::default();
        unsafe { f_add(&[num(5), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    // --- f_insert ---

    #[test]
    fn insert_into_a_list_at_the_start_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, 2) };
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(1)];
        unsafe { f_insert(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(l, list);
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 2);
            let item = crate::eval::typval::tv_list_first(l);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(1));
            let item2 = (*item).li_next;
            assert_eq!((*item2).li_tv.value, TypvalValue::Number(2));
            crate::eval::typval::tv_list_unref(l);
        }
    }

    #[test]
    fn insert_into_a_list_before_a_given_index() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 3);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(2), num(1)];
        unsafe { f_insert(&args, &mut rettv) };
        unsafe {
            let item = crate::eval::typval::tv_list_first(list);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(1));
            let item2 = (*item).li_next;
            assert_eq!((*item2).li_tv.value, TypvalValue::Number(2));
            let item3 = (*item2).li_next;
            assert_eq!((*item3).li_tv.value, TypvalValue::Number(3));
            crate::eval::typval::tv_list_unref(list);
        }
    }

    #[test]
    fn insert_at_the_end_when_idx_equals_length() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, 1) };
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(2), num(1)];
        unsafe { f_insert(&args, &mut rettv) };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(list), 2);
            let item = crate::eval::typval::tv_list_first(list);
            let item2 = (*item).li_next;
            assert_eq!((*item2).li_tv.value, TypvalValue::Number(2));
            crate::eval::typval::tv_list_unref(list);
        }
    }

    #[test]
    fn insert_into_a_locked_list_leaves_it_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        unsafe { (*list).lv_lock = crate::eval::typval_defs::VarLockStatus::Locked };
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(1)];
        unsafe { f_insert(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999)); // untouched.
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(list), 0);
            crate::eval::typval::tv_list_unref(list);
        }
    }

    #[test]
    fn insert_with_an_out_of_range_index_leaves_rettv_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(1), num(5)];
        unsafe { f_insert(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn insert_into_a_blob_at_the_start_by_default() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![2, 3];
            (*blob).bv_ga.ga_len = 2;
            (*blob).bv_ga.ga_maxlen = 2;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(1)];
        unsafe { f_insert(&args, &mut rettv) };
        let TypvalValue::Blob(b) = rettv.value else { panic!("expected a Blob") };
        assert_eq!(b, blob);
        unsafe {
            assert_eq!(crate::eval::typval::tv_blob_len(b), 3);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 0), 1);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 1), 2);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 2), 3);
            crate::eval::typval::tv_blob_free(blob);
        }
    }

    #[test]
    fn insert_into_a_blob_before_a_given_index() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![1, 3];
            (*blob).bv_ga.ga_len = 2;
            (*blob).bv_ga.ga_maxlen = 2;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(2), num(1)];
        unsafe { f_insert(&args, &mut rettv) };
        unsafe {
            assert_eq!(crate::eval::typval::tv_blob_len(blob), 3);
            assert_eq!(crate::eval::typval::tv_blob_get(blob, 0), 1);
            assert_eq!(crate::eval::typval::tv_blob_get(blob, 1), 2);
            assert_eq!(crate::eval::typval::tv_blob_get(blob, 2), 3);
            crate::eval::typval::tv_blob_free(blob);
        }
    }

    #[test]
    fn insert_into_a_locked_blob_leaves_it_untouched() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_lock = crate::eval::typval_defs::VarLockStatus::Locked;
            (*blob).bv_ga.ga_data = vec![1];
            (*blob).bv_ga.ga_len = 1;
        }
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(2)];
        unsafe { f_insert(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe {
            assert_eq!(crate::eval::typval::tv_blob_len(blob), 1);
            crate::eval::typval::tv_blob_free(blob);
        }
    }

    #[test]
    fn insert_with_an_out_of_range_blob_value_leaves_rettv_untouched() {
        let blob = crate::eval::typval::tv_blob_alloc();
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(256)];
        unsafe { f_insert(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe { crate::eval::typval::tv_blob_free(blob) };
    }

    #[test]
    fn insert_into_a_null_blob_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::Blob(std::ptr::null_mut()), ..Default::default() }, num(1)];
        unsafe { f_insert(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    #[test]
    fn insert_into_a_non_list_non_blob_leaves_rettv_at_default() {
        let mut rettv = TypvalT { value: TypvalValue::Number(0), ..Default::default() };
        unsafe { f_insert(&[num(5), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    // --- f_remove ---
    //
    // f_remove is a thin dispatch wrapper over
    // crate::eval::typval::tv_list_remove/tv_blob_remove/tv_dict_remove,
    // each already thoroughly tested directly in typval.rs's own test
    // module - these tests focus on confirming the dispatch itself
    // routes to the right one, not re-covering every edge case again.

    #[test]
    fn remove_dispatches_to_a_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 2);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(0)];
        unsafe { f_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(list), 1);
            crate::eval::typval::tv_list_free(list);
        }
    }

    #[test]
    fn remove_dispatches_to_a_blob() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![10, 20];
            (*blob).bv_ga.ga_len = 2;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(0)];
        unsafe { f_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(10));
        unsafe {
            assert_eq!(crate::eval::typval::tv_blob_len(blob), 1);
            crate::eval::typval::tv_blob_free(blob);
        }
    }

    #[test]
    fn remove_dispatches_to_a_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe { crate::eval::typval::tv_dict_add_nr(&mut *dict, b"a", 42) };
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }, string(b"a")];
        unsafe { f_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(42));
        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn remove_on_a_non_list_non_dict_non_blob_leaves_rettv_at_default() {
        let mut rettv = TypvalT { value: TypvalValue::Number(0), ..Default::default() };
        unsafe { f_remove(&[num(5), num(0)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    // --- f_extend / f_extendnew ---

    #[test]
    fn extend_merges_two_lists_in_place() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *l1, 1);
            crate::eval::typval::tv_list_append_number(&mut *l1, 2);
        }
        let l2 = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *l2, 3);
            crate::eval::typval::tv_list_append_number(&mut *l2, 4);
        }
        let mut rettv = TypvalT::default();
        let args =
            [TypvalT { value: TypvalValue::List(l1), ..Default::default() }, TypvalT { value: TypvalValue::List(l2), ..Default::default() }];
        unsafe { f_extend(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(l, l1); // same list, mutated in place.
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 4);
            let mut vals = Vec::new();
            let mut item = crate::eval::typval::tv_list_first(l);
            while !item.is_null() {
                vals.push((*item).li_tv.value.clone());
                item = (*item).li_next;
            }
            assert_eq!(vals, vec![TypvalValue::Number(1), TypvalValue::Number(2), TypvalValue::Number(3), TypvalValue::Number(4)]);
            crate::eval::typval::tv_list_unref(l1);
            crate::eval::typval::tv_list_unref(l2);
        }
    }

    #[test]
    fn extend_inserts_before_a_given_index() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *l1, 1);
            crate::eval::typval::tv_list_append_number(&mut *l1, 4);
        }
        let l2 = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *l2, 2);
            crate::eval::typval::tv_list_append_number(&mut *l2, 3);
        }
        let mut rettv = TypvalT::default();
        let args = [
            TypvalT { value: TypvalValue::List(l1), ..Default::default() },
            TypvalT { value: TypvalValue::List(l2), ..Default::default() },
            num(1),
        ];
        unsafe { f_extend(&args, &mut rettv) };
        unsafe {
            let mut vals = Vec::new();
            let mut item = crate::eval::typval::tv_list_first(l1);
            while !item.is_null() {
                vals.push((*item).li_tv.value.clone());
                item = (*item).li_next;
            }
            assert_eq!(vals, vec![TypvalValue::Number(1), TypvalValue::Number(2), TypvalValue::Number(3), TypvalValue::Number(4)]);
            crate::eval::typval::tv_list_unref(l1);
            crate::eval::typval::tv_list_unref(l2);
        }
    }

    #[test]
    fn extend_of_a_locked_list_leaves_it_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = crate::eval::typval::tv_list_alloc(0);
        unsafe { (*l1).lv_lock = crate::eval::typval_defs::VarLockStatus::Locked };
        let l2 = crate::eval::typval::tv_list_alloc(0);
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args =
            [TypvalT { value: TypvalValue::List(l1), ..Default::default() }, TypvalT { value: TypvalValue::List(l2), ..Default::default() }];
        unsafe { f_extend(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe {
            crate::eval::typval::tv_list_unref(l1);
            crate::eval::typval::tv_list_unref(l2);
        }
    }

    #[test]
    fn extend_merges_two_dicts_with_default_force_action() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = crate::eval::typval::tv_dict_alloc();
        unsafe { crate::eval::typval::tv_dict_add_nr(&mut *d1, b"a", 1) };
        let d2 = crate::eval::typval::tv_dict_alloc();
        unsafe {
            crate::eval::typval::tv_dict_add_nr(&mut *d2, b"a", 2);
            crate::eval::typval::tv_dict_add_nr(&mut *d2, b"b", 3);
        }
        let mut rettv = TypvalT::default();
        let args =
            [TypvalT { value: TypvalValue::Dict(d1), ..Default::default() }, TypvalT { value: TypvalValue::Dict(d2), ..Default::default() }];
        unsafe { f_extend(&args, &mut rettv) };
        let TypvalValue::Dict(d) = rettv.value else { panic!("expected a Dict") };
        assert_eq!(d, d1);
        unsafe {
            let a = crate::eval::typval::tv_dict_find(Some(&mut *d1), b"a").unwrap();
            assert_eq!((*a).di_tv.value, TypvalValue::Number(2)); // overwritten by "force".
            let b = crate::eval::typval::tv_dict_find(Some(&mut *d1), b"b").unwrap();
            assert_eq!((*b).di_tv.value, TypvalValue::Number(3));
            crate::eval::typval::tv_dict_unref(d1);
            crate::eval::typval::tv_dict_unref(d2);
        }
    }

    #[test]
    fn extend_dict_with_keep_action_preserves_existing_values() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = crate::eval::typval::tv_dict_alloc();
        unsafe { crate::eval::typval::tv_dict_add_nr(&mut *d1, b"a", 1) };
        let d2 = crate::eval::typval::tv_dict_alloc();
        unsafe {
            crate::eval::typval::tv_dict_add_nr(&mut *d2, b"a", 2);
            crate::eval::typval::tv_dict_add_nr(&mut *d2, b"b", 3);
        }
        let mut rettv = TypvalT::default();
        let args = [
            TypvalT { value: TypvalValue::Dict(d1), ..Default::default() },
            TypvalT { value: TypvalValue::Dict(d2), ..Default::default() },
            string(b"keep"),
        ];
        unsafe { f_extend(&args, &mut rettv) };
        unsafe {
            let a = crate::eval::typval::tv_dict_find(Some(&mut *d1), b"a").unwrap();
            assert_eq!((*a).di_tv.value, TypvalValue::Number(1)); // kept, not overwritten.
            let b = crate::eval::typval::tv_dict_find(Some(&mut *d1), b"b").unwrap();
            assert_eq!((*b).di_tv.value, TypvalValue::Number(3)); // still added.
            crate::eval::typval::tv_dict_unref(d1);
            crate::eval::typval::tv_dict_unref(d2);
        }
    }

    #[test]
    fn extend_dict_with_error_action_and_a_duplicate_key_leaves_the_value_unchanged() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = crate::eval::typval::tv_dict_alloc();
        unsafe { crate::eval::typval::tv_dict_add_nr(&mut *d1, b"a", 1) };
        let d2 = crate::eval::typval::tv_dict_alloc();
        unsafe { crate::eval::typval::tv_dict_add_nr(&mut *d2, b"a", 2) };
        let mut rettv = TypvalT::default();
        let args = [
            TypvalT { value: TypvalValue::Dict(d1), ..Default::default() },
            TypvalT { value: TypvalValue::Dict(d2), ..Default::default() },
            string(b"error"),
        ];
        unsafe { f_extend(&args, &mut rettv) };
        unsafe {
            let a = crate::eval::typval::tv_dict_find(Some(&mut *d1), b"a").unwrap();
            assert_eq!((*a).di_tv.value, TypvalValue::Number(1));
            crate::eval::typval::tv_dict_unref(d1);
            crate::eval::typval::tv_dict_unref(d2);
        }
    }

    #[test]
    fn extend_of_a_locked_dict_leaves_it_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = crate::eval::typval::tv_dict_alloc();
        unsafe { (*d1).dv_lock = crate::eval::typval_defs::VarLockStatus::Locked };
        let d2 = crate::eval::typval::tv_dict_alloc();
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args =
            [TypvalT { value: TypvalValue::Dict(d1), ..Default::default() }, TypvalT { value: TypvalValue::Dict(d2), ..Default::default() }];
        unsafe { f_extend(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe {
            crate::eval::typval::tv_dict_unref(d1);
            crate::eval::typval::tv_dict_unref(d2);
        }
    }

    #[test]
    fn extend_with_mismatched_types_leaves_rettv_at_default() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = crate::eval::typval::tv_list_alloc(0);
        let d2 = crate::eval::typval::tv_dict_alloc();
        let mut rettv = TypvalT { value: TypvalValue::Number(0), ..Default::default() };
        let args =
            [TypvalT { value: TypvalValue::List(l1), ..Default::default() }, TypvalT { value: TypvalValue::Dict(d2), ..Default::default() }];
        unsafe { f_extend(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        unsafe {
            crate::eval::typval::tv_list_unref(l1);
            crate::eval::typval::tv_dict_unref(d2);
        }
    }

    #[test]
    fn extend_dict_with_an_invalid_action_string_leaves_rettv_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = crate::eval::typval::tv_dict_alloc();
        let d2 = crate::eval::typval::tv_dict_alloc();
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [
            TypvalT { value: TypvalValue::Dict(d1), ..Default::default() },
            TypvalT { value: TypvalValue::Dict(d2), ..Default::default() },
            string(b"bogus"),
        ];
        unsafe { f_extend(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe {
            crate::eval::typval::tv_dict_unref(d1);
            crate::eval::typval::tv_dict_unref(d2);
        }
    }

    #[test]
    fn extendnew_returns_a_new_list_leaving_the_original_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *l1, 1);
            crate::eval::typval::tv_list_append_number(&mut *l1, 2);
        }
        let l2 = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_append_number(&mut *l2, 3) };
        let mut rettv = TypvalT::default();
        let args =
            [TypvalT { value: TypvalValue::List(l1), ..Default::default() }, TypvalT { value: TypvalValue::List(l2), ..Default::default() }];
        unsafe { f_extendnew(&args, &mut rettv) };
        let TypvalValue::List(new_list) = rettv.value else { panic!("expected a List") };
        assert_ne!(new_list, l1); // a genuinely separate list.
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(new_list), 3);
            assert_eq!(crate::eval::typval::tv_list_len(l1), 2); // original untouched.
            crate::eval::typval::tv_list_unref(l1);
            crate::eval::typval::tv_list_unref(l2);
            crate::eval::typval::tv_list_unref(new_list);
        }
    }

    #[test]
    fn extendnew_returns_a_new_dict_leaving_the_original_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = crate::eval::typval::tv_dict_alloc();
        unsafe { crate::eval::typval::tv_dict_add_nr(&mut *d1, b"a", 1) };
        let d2 = crate::eval::typval::tv_dict_alloc();
        unsafe { crate::eval::typval::tv_dict_add_nr(&mut *d2, b"b", 2) };
        let mut rettv = TypvalT::default();
        let args =
            [TypvalT { value: TypvalValue::Dict(d1), ..Default::default() }, TypvalT { value: TypvalValue::Dict(d2), ..Default::default() }];
        unsafe { f_extendnew(&args, &mut rettv) };
        let TypvalValue::Dict(new_dict) = rettv.value else { panic!("expected a Dict") };
        assert_ne!(new_dict, d1);
        unsafe {
            assert_eq!(crate::eval::typval::tv_dict_len(new_dict.as_ref()), 2);
            assert_eq!(crate::eval::typval::tv_dict_len(d1.as_ref()), 1); // original untouched.
            crate::eval::typval::tv_dict_unref(d1);
            crate::eval::typval::tv_dict_unref(d2);
            crate::eval::typval::tv_dict_unref(new_dict);
        }
    }

    // --- f_range ---

    fn list_values(l: *mut crate::eval::typval_defs::ListT) -> Vec<crate::eval::typval_defs::VarnumberT> {
        let mut vals = Vec::new();
        unsafe {
            let mut item = crate::eval::typval::tv_list_first(l);
            while !item.is_null() {
                let TypvalValue::Number(n) = (*item).li_tv.value else { panic!("expected a Number") };
                vals.push(n);
                item = (*item).li_next;
            }
        }
        vals
    }

    #[test]
    fn range_with_one_argument_produces_zero_to_n_minus_one() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_range(&[num(4)], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(list_values(l), vec![0, 1, 2, 3]);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn range_with_one_argument_of_zero_produces_an_empty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_range(&[num(0)], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(list_values(l), Vec::<crate::eval::typval_defs::VarnumberT>::new());
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn range_with_two_arguments_is_inclusive_of_the_end() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_range(&[num(2), num(5)], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(list_values(l), vec![2, 3, 4, 5]);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn range_with_a_stride_steps_by_that_amount() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_range(&[num(0), num(10), num(3)], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(list_values(l), vec![0, 3, 6, 9]);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn range_with_a_negative_stride_counts_down() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_range(&[num(5), num(1), num(-2)], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(list_values(l), vec![5, 3, 1]);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn range_with_a_zero_stride_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        unsafe { f_range(&[num(0), num(5), num(0)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    #[test]
    fn range_with_start_past_end_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        unsafe { f_range(&[num(5), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    // --- f_repeat ---

    #[test]
    fn repeat_string_repeats_n_times() {
        let mut rettv = TypvalT::default();
        unsafe { f_repeat(&[string(b"ab"), num(3)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"ababab".to_vec())));
    }

    #[test]
    fn repeat_string_with_zero_or_negative_count_is_empty() {
        let mut rettv = TypvalT::default();
        unsafe { f_repeat(&[string(b"ab"), num(0)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
        unsafe { f_repeat(&[string(b"ab"), num(-1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    #[test]
    fn repeat_string_stops_at_an_embedded_nul() {
        let mut rettv = TypvalT::default();
        unsafe { f_repeat(&[string(b"ab\0cd"), num(2)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"abab".to_vec())));
    }

    #[test]
    fn repeat_of_a_number_stringifies_first() {
        let mut rettv = TypvalT::default();
        unsafe { f_repeat(&[num(12), num(2)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"1212".to_vec())));
    }

    #[test]
    fn repeat_list_repeats_n_times() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 2);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(3)];
        unsafe { f_repeat(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_ne!(l, list);
        unsafe {
            assert_eq!(list_values(l), vec![1, 2, 1, 2, 1, 2]);
            crate::eval::typval::tv_list_unref(list);
            crate::eval::typval::tv_list_unref(l);
        }
    }

    #[test]
    fn repeat_list_with_zero_count_is_an_empty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, 1) };
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(0)];
        unsafe { f_repeat(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 0);
            crate::eval::typval::tv_list_unref(list);
            crate::eval::typval::tv_list_unref(l);
        }
    }

    #[test]
    fn repeat_blob_repeats_n_times() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![1, 2];
            (*blob).bv_ga.ga_len = 2;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(2)];
        unsafe { f_repeat(&args, &mut rettv) };
        let TypvalValue::Blob(b) = rettv.value else { panic!("expected a Blob") };
        assert_ne!(b, blob);
        unsafe {
            assert_eq!(crate::eval::typval::tv_blob_len(b), 4);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 0), 1);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 1), 2);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 2), 1);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 3), 2);
            crate::eval::typval::tv_blob_free(blob);
            crate::eval::typval::tv_blob_free(b);
        }
    }

    #[test]
    fn repeat_blob_of_a_null_blob_is_an_empty_blob() {
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(std::ptr::null_mut()), ..Default::default() }, num(3)];
        unsafe { f_repeat(&args, &mut rettv) };
        let TypvalValue::Blob(b) = rettv.value else { panic!("expected a Blob") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_blob_len(b), 0);
            crate::eval::typval::tv_blob_free(b);
        }
    }

    // --- f_join ---

    #[test]
    fn join_with_default_separator_uses_a_space() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 2);
            crate::eval::typval::tv_list_append_number(&mut *list, 3);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_join(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"1 2 3".to_vec())));
        unsafe { crate::eval::typval::tv_list_free(list) };
    }

    #[test]
    fn join_with_a_custom_separator() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_string(list, Some(b"a"));
            crate::eval::typval::tv_list_append_string(list, Some(b"b"));
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, string(b", ")];
        unsafe { f_join(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"a, b".to_vec())));
        unsafe { crate::eval::typval::tv_list_free(list) };
    }

    #[test]
    fn join_of_a_non_list_leaves_rettv_at_default() {
        let mut rettv = TypvalT { value: TypvalValue::Number(0), ..Default::default() };
        unsafe { f_join(&[num(5)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn join_with_a_type_error_separator_sets_a_null_string() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let inner_sep = crate::eval::typval::tv_list_alloc(0);
        let args = [
            TypvalT { value: TypvalValue::List(list), ..Default::default() },
            TypvalT { value: TypvalValue::List(inner_sep), ..Default::default() },
        ];
        unsafe { f_join(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
        unsafe {
            crate::eval::typval::tv_list_free(list);
            crate::eval::typval::tv_list_free(inner_sep);
        }
    }

    // --- f_flatten / f_flattennew ---

    #[test]
    fn flatten_flattens_in_place_and_returns_the_same_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(2);
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, 1) };
        let inner = crate::eval::typval::tv_list_alloc(1);
        unsafe {
            crate::eval::typval::tv_list_ref(inner);
            crate::eval::typval::tv_list_append_number(&mut *inner, 2);
            crate::eval::typval::tv_list_append_owned_tv(
                list,
                TypvalT { value: TypvalValue::List(inner), ..Default::default() },
            );
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_flatten(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(l, list);
        unsafe {
            assert_eq!(list_values(l), vec![1, 2]);
            crate::eval::typval::tv_list_unref(l);
        }
    }

    #[test]
    fn flatten_of_a_locked_list_leaves_it_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        unsafe { (*list).lv_lock = crate::eval::typval_defs::VarLockStatus::Locked };
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_flatten(&args, &mut rettv) };
        // rettv still gets the List value assigned upfront (matching
        // the original's own unconditional rettv assignment before
        // the lock check), even though flattening itself never runs.
        assert_eq!(rettv.value, TypvalValue::List(list));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn flatten_with_a_negative_maxdepth_leaves_rettv_at_default() {
        let mut rettv = TypvalT { value: TypvalValue::Number(0), ..Default::default() };
        let list = crate::eval::typval::tv_list_alloc(0);
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(-1)];
        unsafe { f_flatten(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        unsafe { crate::eval::typval::tv_list_free(list) };
    }

    #[test]
    fn flatten_of_a_non_list_leaves_rettv_at_default() {
        let mut rettv = TypvalT { value: TypvalValue::Number(0), ..Default::default() };
        unsafe { f_flatten(&[num(5)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn flattennew_returns_a_new_list_leaving_the_original_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        let inner = crate::eval::typval::tv_list_alloc(1);
        unsafe {
            crate::eval::typval::tv_list_ref(inner);
            crate::eval::typval::tv_list_append_number(&mut *inner, 1);
            crate::eval::typval::tv_list_append_owned_tv(
                list,
                TypvalT { value: TypvalValue::List(inner), ..Default::default() },
            );
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_flattennew(&args, &mut rettv) };
        let TypvalValue::List(new_list) = rettv.value else { panic!("expected a List") };
        assert_ne!(new_list, list);
        unsafe {
            assert_eq!(list_values(new_list), vec![1]);
            assert_eq!(crate::eval::typval::tv_list_len(list), 1); // original still nested, untouched.
            let orig_item = crate::eval::typval::tv_list_first(list);
            assert!(matches!((*orig_item).li_tv.value, TypvalValue::List(_)));
            crate::eval::typval::tv_list_unref(list);
            crate::eval::typval::tv_list_unref(new_list);
        }
    }

    // --- f_localtime ---

    #[test]
    fn localtime_returns_a_positive_unix_timestamp() {
        let mut rettv = TypvalT::default();
        f_localtime(&[], &mut rettv);
        let TypvalValue::Number(n) = rettv.value else { panic!("expected a Number") };
        // Any real wall-clock time is comfortably past 2020-01-01
        // (1577836800) - a loose sanity bound, not a flaky exact-time
        // check.
        assert!(n > 1_577_836_800);
    }

    // --- f_getenv / f_environ ---
    //
    // Each test below uses a uniquely-named test-only environment
    // variable (never a well-known name like PATH/HOME/VIM) so it
    // cannot race with any other test in the whole crate that also
    // touches process-wide environment state, regardless of parallel
    // execution - avoiding the need for a shared cross-module lock.

    #[test]
    fn getenv_returns_the_value_of_a_set_variable() {
        // SAFETY: NERO_TEST_GETENV_UNIQUE_VAR is unique to this test.
        unsafe { std::env::set_var("NERO_TEST_GETENV_UNIQUE_VAR", "hello") };
        let mut rettv = TypvalT::default();
        unsafe { f_getenv(&[string(b"NERO_TEST_GETENV_UNIQUE_VAR")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hello".to_vec())));
        // SAFETY: forwarded from the set_var call above.
        unsafe { std::env::remove_var("NERO_TEST_GETENV_UNIQUE_VAR") };
    }

    #[test]
    fn getenv_returns_null_for_an_unset_variable() {
        let mut rettv = TypvalT::default();
        unsafe { f_getenv(&[string(b"NERO_TEST_GETENV_DEFINITELY_UNSET_VAR")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null));
    }

    #[test]
    fn environ_returns_a_non_empty_dict() {
        // Missing this lock was the ACTUAL root cause of a
        // hashtab-capacity panic reproduced on native Linux (not the
        // env-mutation hazard suspected initially, which was a real
        // but separate concern also worth avoiding - see below):
        // tv_dict_alloc touches the crate-wide GC_FIRST_DICT linked
        // list, so running concurrently with another dict-touching
        // test (without this lock serializing them) is a genuine data
        // race on that shared global list - this was caught by
        // multiple_dicts_maintain_the_gc_linked_list_correctly also
        // failing deterministically alongside this test on native
        // Linux (never reproduced on Windows, latency/scheduling-
        // dependent like most data races).
        let _lock = crate::globals::global_state_test_lock();
        // Also deliberately does NOT mutate the environment (no
        // set_var/remove_var) - environ()'s own full enumeration is
        // separately not safely reentrant against ANY concurrent
        // env-var mutation from another thread (a well-known,
        // platform-specific hazard, worse on Linux/glibc than
        // Windows). Checking for a non-empty Dict (rather than
        // mutating and checking a specific fresh key) avoids that
        // hazard too, while still verifying real enumeration
        // happened.
        let mut rettv = TypvalT::default();
        f_environ(&[], &mut rettv);
        let TypvalValue::Dict(d) = rettv.value else { panic!("expected a Dict") };
        unsafe {
            assert!(crate::eval::typval::tv_dict_len(d.as_ref()) > 0);
            crate::eval::typval::tv_dict_unref(d);
        }
    }

    // --- f_setenv ---
    //
    // Each test uses a uniquely-named test-only environment variable
    // (never a well-known name) - see f_getenv/f_environ's own comment
    // above. Also holds global_state_test_lock() for the same reason
    // environ_returns_a_non_empty_dict does: mutating the environment
    // (via set_var/remove_var, reached through vim_setenv_ext/
    // vim_unsetenv_ext) is not safely reentrant against a concurrent
    // full-environment enumeration on some platforms.

    #[test]
    fn setenv_sets_an_environment_variable() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let mut rettv = TypvalT::default();
        unsafe {
            f_setenv(&[string(b"NERO_TEST_SETENV_UNIQUE_VAR"), string(b"hello")], &mut rettv);
        }
        assert_eq!(crate::os::env::os_getenv(b"NERO_TEST_SETENV_UNIQUE_VAR"), Some(b"hello".to_vec()));
        // rettv is untouched (void function) - stays the Default (Unknown).
        assert!(matches!(rettv.value, TypvalValue::Unknown));

        // SAFETY: this test's own unique variable.
        unsafe { crate::os::env::os_unsetenv(b"NERO_TEST_SETENV_UNIQUE_VAR") };
    }

    #[test]
    fn setenv_with_null_value_unsets_the_variable() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        // SAFETY: this test's own unique variable.
        unsafe { crate::os::env::os_setenv(b"NERO_TEST_SETENV_NULL_VAR", b"hello", 1) };

        let null_tv = TypvalT { value: TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null), ..Default::default() };
        let mut rettv = TypvalT::default();
        unsafe { f_setenv(&[string(b"NERO_TEST_SETENV_NULL_VAR"), null_tv], &mut rettv) };

        assert_eq!(crate::os::env::os_getenv(b"NERO_TEST_SETENV_NULL_VAR"), None);
    }

    #[test]
    fn setenv_fails_silently_and_secure_is_bumped_when_secure_is_set() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 1;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let mut rettv = TypvalT::default();
        unsafe {
            f_setenv(&[string(b"NERO_TEST_SETENV_SECURE_VAR"), string(b"hello")], &mut rettv);
        }
        assert_eq!(crate::os::env::os_getenv(b"NERO_TEST_SETENV_SECURE_VAR"), None);
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.secure, 2);
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
    }

    // --- f_changenr ---

    #[test]
    fn changenr_reads_curbuf_b_u_seq_cur() {
        let mut buf = crate::buffer_defs::BufT { b_u_seq_cur: 42, ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);

        let mut rettv = TypvalT::default();
        unsafe { f_changenr(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(42));
    }

    // --- f_interrupt ---

    #[test]
    fn interrupt_sets_got_int() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.got_int = false;

        let mut rettv = TypvalT::default();
        unsafe { f_interrupt(&[], &mut rettv) };
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.got_int);

        unsafe { crate::globals::GLOBALS.get_mut() }.got_int = false;
    }

    // --- f_invert ---

    #[test]
    fn invert_bitwise_nots_a_number() {
        let mut rettv = TypvalT::default();
        f_invert(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-1));

        let mut rettv = TypvalT::default();
        f_invert(&[num(5)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(!5));
    }

    // --- f_getfontname ---

    #[test]
    fn getfontname_always_returns_an_empty_string() {
        let mut rettv = TypvalT::default();
        f_getfontname(&[], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::String(None));

        // Even with an argument - the GUI-less stub ignores it.
        let mut rettv = TypvalT::default();
        f_getfontname(&[string(b"Courier")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    // --- f_isinf ---

    #[test]
    fn isinf_returns_1_for_positive_infinity() {
        let mut rettv = TypvalT::default();
        f_isinf(&[float(f64::INFINITY)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn isinf_returns_minus_1_for_negative_infinity() {
        let mut rettv = TypvalT::default();
        f_isinf(&[float(f64::NEG_INFINITY)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    #[test]
    fn isinf_leaves_rettv_default_for_a_finite_float() {
        let mut rettv = TypvalT::default();
        f_isinf(&[float(1.5)], &mut rettv);
        assert!(matches!(rettv.value, TypvalValue::Unknown));
    }

    #[test]
    fn isinf_leaves_rettv_default_for_a_non_float() {
        let mut rettv = TypvalT::default();
        f_isinf(&[num(5)], &mut rettv);
        assert!(matches!(rettv.value, TypvalValue::Unknown));
    }

    // --- f_isnan ---

    #[test]
    fn isnan_true_for_nan_float() {
        let mut rettv = TypvalT::default();
        f_isnan(&[float(f64::NAN)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn isnan_false_for_an_ordinary_float() {
        let mut rettv = TypvalT::default();
        f_isnan(&[float(1.5)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn isnan_false_for_a_non_float() {
        let mut rettv = TypvalT::default();
        f_isnan(&[num(5)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    // --- f_sha256 ---

    #[test]
    fn sha256_of_a_known_string() {
        // Known SHA256("abc") test vector.
        let mut rettv = TypvalT::default();
        unsafe { f_sha256(&[string(b"abc")], &mut rettv) };
        assert_eq!(
            rettv.value,
            TypvalValue::String(Some(b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_vec()))
        );
    }

    #[test]
    fn sha256_of_a_blob() {
        let b = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*b).bv_ga.ga_data = b"abc".to_vec();
            (*b).bv_ga.ga_len = 3;
        }
        let mut rettv = TypvalT::default();
        unsafe { f_sha256(&[TypvalT { value: TypvalValue::Blob(b), ..Default::default() }], &mut rettv) };
        assert_eq!(
            rettv.value,
            TypvalValue::String(Some(b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_vec()))
        );
        unsafe { crate::eval::typval::tv_blob_free(b) };
    }

    #[test]
    fn sha256_of_a_null_blob_matches_empty_string() {
        let mut rettv = TypvalT::default();
        unsafe { f_sha256(&[TypvalT { value: TypvalValue::Blob(std::ptr::null_mut()), ..Default::default() }], &mut rettv) };
        let TypvalValue::String(Some(hash)) = rettv.value else { panic!("expected a String") };

        let mut rettv2 = TypvalT::default();
        unsafe { f_sha256(&[string(b"")], &mut rettv2) };
        assert_eq!(rettv2.value, TypvalValue::String(Some(hash)));
    }

    // --- f_exists ---

    #[test]
    fn exists_true_for_a_defined_option() {
        let mut rettv = TypvalT::default();
        unsafe { f_exists(&[string(b"&ignorecase")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn exists_false_for_an_unknown_option() {
        let mut rettv = TypvalT::default();
        unsafe { f_exists(&[string(b"&notarealoption")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn exists_false_for_an_option_with_trailing_garbage() {
        let mut rettv = TypvalT::default();
        unsafe { f_exists(&[string(b"&ignorecase extra")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn exists_true_for_a_defined_global_variable() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::eval::vars::vars_clear(&mut *crate::eval::vars::get_globvar_dict()) };

        let item = crate::eval::typval::tv_dict_item_alloc(b"nero_test_exists_var");
        unsafe { (*item).di_tv.value = TypvalValue::Number(7) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *crate::eval::vars::get_globvar_dict(), item) };

        let mut rettv = TypvalT::default();
        unsafe { f_exists(&[string(b"g:nero_test_exists_var")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));

        unsafe { crate::eval::vars::vars_clear(&mut *crate::eval::vars::get_globvar_dict()) };
    }

    #[test]
    fn exists_false_for_an_undefined_variable() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::eval::vars::vars_clear(&mut *crate::eval::vars::get_globvar_dict()) };

        let mut rettv = TypvalT::default();
        unsafe { f_exists(&[string(b"g:definitely_not_defined")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn exists_true_for_a_set_environment_variable() {
        // SAFETY: unique test-only variable name.
        unsafe { crate::os::env::os_setenv(b"NERO_TEST_EXISTS_ENV_VAR", b"hello", 1) };

        let mut rettv = TypvalT::default();
        unsafe { f_exists(&[string(b"$NERO_TEST_EXISTS_ENV_VAR")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));

        unsafe { crate::os::env::os_unsetenv(b"NERO_TEST_EXISTS_ENV_VAR") };
    }

    #[test]
    fn exists_false_for_an_unset_environment_variable() {
        let mut rettv = TypvalT::default();
        unsafe { f_exists(&[string(b"$NERO_TEST_EXISTS_DEFINITELY_UNSET")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn exists_function_branch_is_unimplemented() {
        let mut rettv = TypvalT::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            f_exists(&[string(b"*Foo")], &mut rettv);
        }));
        assert!(result.is_err(), "expected a panic (function_exists not yet translated)");
    }

    #[test]
    fn exists_command_branch_is_unimplemented() {
        let mut rettv = TypvalT::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            f_exists(&[string(b":Foo")], &mut rettv);
        }));
        assert!(result.is_err(), "expected a panic (cmd_exists not yet translated)");
    }

    #[test]
    fn exists_autocmd_branch_is_unimplemented() {
        let mut rettv = TypvalT::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            f_exists(&[string(b"#BufEnter")], &mut rettv);
        }));
        assert!(result.is_err(), "expected a panic (au_exists not yet translated)");
    }

    // --- f_getwinpos / f_getwinposx / f_getwinposy ---

    #[test]
    fn getwinpos_always_returns_minus_1_minus_1() {
        let mut rettv = TypvalT::default();
        unsafe { f_getwinpos(&[], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 2);
            let first = crate::eval::typval::tv_list_find(l, 0);
            let second = crate::eval::typval::tv_list_find(l, 1);
            assert_eq!((*first).li_tv.value, TypvalValue::Number(-1));
            assert_eq!((*second).li_tv.value, TypvalValue::Number(-1));
            crate::eval::typval::tv_list_unref(l);
        }
    }

    #[test]
    fn getwinposx_always_returns_minus_1() {
        let mut rettv = TypvalT::default();
        f_getwinposx(&[], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    #[test]
    fn getwinposy_always_returns_minus_1() {
        let mut rettv = TypvalT::default();
        f_getwinposy(&[], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    // --- f_win_getid / f_win_id2win / f_win_id2tabwin ---

    /// Points `GLOBALS.firstwin`/`curtab`/`curwin`/`first_tabpage` at
    /// the given values for the guard's lifetime, restoring all
    /// previous values on drop - a `funcs.rs`-local copy of
    /// `window.rs`'s own private `CurwinListGuard`/`FirstTabpageGuard`
    /// test fixtures (that module's own version is private to its own
    /// test module, so not reusable directly from here).
    struct WinGlobalsGuard {
        prev_firstwin: *mut crate::buffer_defs::WinT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
        prev_curwin: *mut crate::buffer_defs::WinT,
        prev_first_tabpage: *mut crate::buffer_defs::TabpageT,
    }

    impl WinGlobalsGuard {
        fn set(win: *mut crate::buffer_defs::WinT, tp: *mut crate::buffer_defs::TabpageT) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = WinGlobalsGuard {
                prev_firstwin: globals.firstwin,
                prev_curtab: globals.curtab,
                prev_curwin: globals.curwin,
                prev_first_tabpage: globals.first_tabpage,
            };
            globals.firstwin = win;
            globals.curtab = tp;
            globals.curwin = win;
            globals.first_tabpage = tp;
            guard
        }
    }

    impl Drop for WinGlobalsGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.firstwin = self.prev_firstwin;
            globals.curtab = self.prev_curtab;
            globals.curwin = self.prev_curwin;
            globals.first_tabpage = self.prev_first_tabpage;
        }
    }

    fn focusable_win(handle: crate::types_defs::HandleT) -> crate::buffer_defs::WinT {
        crate::buffer_defs::WinT {
            handle,
            w_config: crate::buffer_defs::WinConfig { focusable: true, hide: false, ..Default::default() },
            ..Default::default()
        }
    }

    #[test]
    fn win_getid_with_no_args_returns_curwin_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(77);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WinGlobalsGuard::set(&mut win, &mut tp);

        let mut rettv = TypvalT::default();
        unsafe { f_win_getid(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(77));
    }

    #[test]
    fn win_id2win_finds_the_window_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(5);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WinGlobalsGuard::set(&mut win, &mut tp);

        let mut rettv = TypvalT::default();
        unsafe { f_win_id2win(&[num(5)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn win_id2win_returns_0_for_an_unknown_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(5);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WinGlobalsGuard::set(&mut win, &mut tp);

        let mut rettv = TypvalT::default();
        unsafe { f_win_id2win(&[num(999)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn win_id2tabwin_returns_a_list_of_tabnr_and_winnr() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(9);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WinGlobalsGuard::set(&mut win, &mut tp);

        let mut rettv = TypvalT::default();
        unsafe { f_win_id2tabwin(&[num(9)], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 2);
            let tabnr = crate::eval::typval::tv_list_find(l, 0);
            let winnr = crate::eval::typval::tv_list_find(l, 1);
            assert_eq!((*tabnr).li_tv.value, TypvalValue::Number(1));
            assert_eq!((*winnr).li_tv.value, TypvalValue::Number(1));
            crate::eval::typval::tv_list_unref(l);
        }
    }

    #[test]
    fn win_id2tabwin_returns_0_0_for_an_unknown_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(9);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WinGlobalsGuard::set(&mut win, &mut tp);

        let mut rettv = TypvalT::default();
        unsafe { f_win_id2tabwin(&[num(999)], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        unsafe {
            let tabnr = crate::eval::typval::tv_list_find(l, 0);
            let winnr = crate::eval::typval::tv_list_find(l, 1);
            assert_eq!((*tabnr).li_tv.value, TypvalValue::Number(0));
            assert_eq!((*winnr).li_tv.value, TypvalValue::Number(0));
            crate::eval::typval::tv_list_unref(l);
        }
    }

    #[test]
    fn win_findbuf_returns_a_list_of_matching_window_ids() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { handle: 42, ..Default::default() };
        let mut win = crate::buffer_defs::WinT { w_buffer: &mut buf as *mut crate::buffer_defs::BufT, ..focusable_win(9) };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WinGlobalsGuard::set(&mut win, &mut tp);

        let mut rettv = TypvalT::default();
        unsafe { f_win_findbuf(&[num(42)], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 1);
            let item = crate::eval::typval::tv_list_find(l, 0);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(9));
            crate::eval::typval::tv_list_unref(l);
        }
    }

    #[test]
    fn win_findbuf_returns_an_empty_list_for_an_unknown_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { handle: 42, ..Default::default() };
        let mut win = crate::buffer_defs::WinT { w_buffer: &mut buf as *mut crate::buffer_defs::BufT, ..focusable_win(9) };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WinGlobalsGuard::set(&mut win, &mut tp);

        let mut rettv = TypvalT::default();
        unsafe { f_win_findbuf(&[num(999)], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 0);
            crate::eval::typval::tv_list_unref(l);
        }
    }

    #[test]
    fn winnr_with_no_args_returns_the_current_window_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WinGlobalsGuard::set(&mut win, &mut tp);

        let mut rettv = TypvalT::default();
        unsafe { f_winnr(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn winnr_with_dollar_arg_returns_the_last_window_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = focusable_win(2);
        let second_ptr = &mut second as *mut crate::buffer_defs::WinT;
        let mut first = crate::buffer_defs::WinT { w_next: second_ptr, ..focusable_win(1) };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let first_ptr = &mut first as *mut crate::buffer_defs::WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = globals.firstwin;
        let prev_curtab = globals.curtab;
        let prev_curwin = globals.curwin;
        let prev_lastwin = globals.lastwin;
        globals.firstwin = first_ptr;
        globals.curtab = tp_ptr;
        globals.curwin = first_ptr;
        globals.lastwin = second_ptr;

        let mut rettv = TypvalT::default();
        unsafe { f_winnr(&[string(b"$")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(2));

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.firstwin = prev_firstwin;
        globals.curtab = prev_curtab;
        globals.curwin = prev_curwin;
        globals.lastwin = prev_lastwin;
    }

    // --- f_has ---

    #[test]
    fn has_finds_an_always_present_feature() {
        let mut rettv = TypvalT::default();
        f_has(&[string(b"eval")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn has_is_case_insensitive() {
        let mut rettv = TypvalT::default();
        f_has(&[string(b"NVIM")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn has_returns_zero_for_an_unknown_feature() {
        let mut rettv = TypvalT::default();
        f_has(&[string(b"totally_not_a_real_feature")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn has_reports_the_current_platform() {
        let mut rettv = TypvalT::default();
        let name: &[u8] = if cfg!(windows) { b"win32" } else { b"unix" };
        f_has(&[string(name)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn has_does_not_report_the_other_platform() {
        let mut rettv = TypvalT::default();
        let name: &[u8] = if cfg!(windows) { b"unix" } else { b"win32" };
        f_has(&[string(name)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn has_win64_only_matches_on_64_bit_windows() {
        let mut rettv = TypvalT::default();
        f_has(&[string(b"win64")], &mut rettv);
        let expect_true = cfg!(all(windows, target_pointer_width = "64"));
        assert_eq!(rettv.value, TypvalValue::Number(i64::from(expect_true)));
    }

    // --- f_strlen / f_strcharlen / f_strchars / f_strwidth ---

    #[test]
    fn strlen_counts_bytes_not_characters() {
        let mut rettv = TypvalT::default();
        // "一" (U+4E00) is 3 UTF-8 bytes, 1 character.
        f_strlen(&[string("一".as_bytes())], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(3));
    }

    #[test]
    fn strlen_empty_string_is_zero() {
        let mut rettv = TypvalT::default();
        f_strlen(&[string(b"")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn strcharlen_ignores_composing_characters() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        // "e" + COMBINING ACUTE ACCENT is 1 character (composing
        // ignored), unlike strlen's own byte count (3) or strchars'
        // own default (2, composing counted separately).
        let s = "e\u{0301}".as_bytes();
        unsafe { f_strcharlen(&[string(s)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn strchars_default_counts_composing_marks_separately() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let s = "e\u{0301}".as_bytes();
        unsafe { f_strchars(&[string(s)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(2));
    }

    #[test]
    fn strchars_skipcc_matches_strcharlen() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let s = "e\u{0301}".as_bytes();
        unsafe { f_strchars(&[string(s), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn strwidth_counts_a_double_width_char_as_two() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_strwidth(&[string("一".as_bytes())], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(2));
    }

    // --- f_stridx / f_strridx ---

    #[test]
    fn stridx_finds_the_first_occurrence() {
        let mut rettv = TypvalT::default();
        f_stridx(&[string(b"abcabc"), string(b"bc")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn stridx_not_found_is_negative_one() {
        let mut rettv = TypvalT::default();
        f_stridx(&[string(b"abc"), string(b"xyz")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    #[test]
    fn stridx_start_offset_still_reports_index_from_the_beginning() {
        let mut rettv = TypvalT::default();
        f_stridx(&[string(b"abcabc"), string(b"bc"), num(2)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(4));
    }

    #[test]
    fn stridx_start_at_or_past_length_fails() {
        let mut rettv = TypvalT::default();
        f_stridx(&[string(b"abc"), string(b"a"), num(3)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    #[test]
    fn stridx_negative_start_behaves_like_no_start() {
        let mut rettv = TypvalT::default();
        f_stridx(&[string(b"abc"), string(b"a"), num(-5)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn strridx_finds_the_last_occurrence() {
        let mut rettv = TypvalT::default();
        f_strridx(&[string(b"abcabc"), string(b"bc")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(4));
    }

    #[test]
    fn strridx_respects_the_end_index() {
        let mut rettv = TypvalT::default();
        f_strridx(&[string(b"abcabc"), string(b"bc"), num(3)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn strridx_not_found_is_negative_one() {
        let mut rettv = TypvalT::default();
        f_strridx(&[string(b"abc"), string(b"xyz")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    #[test]
    fn strridx_empty_needle_matches_past_the_end() {
        let mut rettv = TypvalT::default();
        f_strridx(&[string(b"abc"), string(b"")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(3));
    }

    // --- f_strgetchar ---

    #[test]
    fn strgetchar_returns_the_character_at_a_character_index() {
        let mut rettv = TypvalT::default();
        f_strgetchar(&[string("a一b".as_bytes()), num(1)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0x4E00));
    }

    #[test]
    fn strgetchar_out_of_range_is_negative_one() {
        let mut rettv = TypvalT::default();
        f_strgetchar(&[string(b"ab"), num(5)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    #[test]
    fn strgetchar_negative_index_is_negative_one() {
        let mut rettv = TypvalT::default();
        f_strgetchar(&[string(b"ab"), num(-1)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    // --- f_strpart ---

    #[test]
    fn strpart_extracts_a_byte_range() {
        let mut rettv = TypvalT::default();
        unsafe { f_strpart(&[string(b"Neovim"), num(2), num(3)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"ovi".to_vec())));
    }

    #[test]
    fn strpart_without_len_takes_everything_after_start() {
        let mut rettv = TypvalT::default();
        unsafe { f_strpart(&[string(b"Neovim"), num(3)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"vim".to_vec())));
    }

    #[test]
    fn strpart_clamps_a_negative_start() {
        let mut rettv = TypvalT::default();
        // start = -2, len = 5 -> len becomes 3, start becomes 0.
        unsafe { f_strpart(&[string(b"hello"), num(-2), num(5)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hel".to_vec())));
    }

    #[test]
    fn strpart_clamps_an_overlong_length() {
        let mut rettv = TypvalT::default();
        unsafe { f_strpart(&[string(b"hi"), num(0), num(100)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hi".to_vec())));
    }

    #[test]
    fn strpart_chars_true_counts_characters_not_bytes() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        // "一二三" is 3 chars, 9 bytes; asking for 2 "characters"
        // starting at byte 0 with chars=true should return the first
        // 2 characters (6 bytes), not the first 2 bytes.
        let tv_true = TypvalT {
            value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True),
            ..Default::default()
        };
        unsafe {
            f_strpart(&[string("一二三".as_bytes()), num(0), num(2), tv_true], &mut rettv);
        }
        assert_eq!(rettv.value, TypvalValue::String(Some("一二".as_bytes().to_vec())));
    }

    // --- f_strtrans ---

    #[test]
    fn strtrans_leaves_printable_ascii_unchanged() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        let mut rettv = TypvalT::default();
        unsafe { f_strtrans(&[string(b"hello")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hello".to_vec())));
    }

    #[test]
    fn strtrans_escapes_a_control_character() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        let mut rettv = TypvalT::default();
        unsafe { f_strtrans(&[string(b"\x01")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"^A".to_vec())));
    }

    // --- f_byteidx / f_byteidxcomp ---

    #[test]
    fn byteidx_ascii_matches_the_character_index() {
        let mut rettv = TypvalT::default();
        unsafe { f_byteidx(&[string(b"hello"), num(2)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(2));
    }

    #[test]
    fn byteidx_skips_past_a_multibyte_character() {
        let mut rettv = TypvalT::default();
        unsafe { f_byteidx(&[string("a一b".as_bytes()), num(2)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(4));
    }

    #[test]
    fn byteidx_out_of_range_is_negative_one() {
        let mut rettv = TypvalT::default();
        unsafe { f_byteidx(&[string(b"ab"), num(5)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    #[test]
    fn byteidx_folds_a_composing_mark_into_the_base_character() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        // "e" + COMBINING ACUTE ACCENT + "x": byteidx (composing
        // folded into the base) skips the whole 3-byte cluster to
        // land on "x" at byte 3.
        let s = "e\u{0301}x".as_bytes();
        unsafe { f_byteidx(&[string(s), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(3));
    }

    #[test]
    fn byteidxcomp_counts_a_composing_mark_as_its_own_character() {
        // byteidxcomp (composing counted separately) lands on the
        // combining mark itself, right after "e", at byte 1.
        let s = "e\u{0301}x".as_bytes();
        let mut rettv = TypvalT::default();
        unsafe { f_byteidxcomp(&[string(s), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    // --- f_charidx ---

    #[test]
    fn charidx_multibyte_string() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        // "a一b": byte 4 is where "b" starts (char index 2).
        unsafe { f_charidx(&[string("a一b".as_bytes()), num(4)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(2));
    }

    #[test]
    fn charidx_exactly_at_the_string_length_returns_the_character_length() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_charidx(&[string(b"abc"), num(3)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(3));
    }

    #[test]
    fn charidx_past_the_string_length_is_negative_one() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_charidx(&[string(b"abc"), num(4)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    #[test]
    fn charidx_negative_index_is_negative_one() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_charidx(&[string(b"abc"), num(-1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    // --- f_strcharpart ---

    #[test]
    fn strcharpart_extracts_a_multibyte_character_by_char_index() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_strcharpart(&[string("a一b".as_bytes()), num(1), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some("一".as_bytes().to_vec())));
    }

    #[test]
    fn strcharpart_without_len_takes_the_rest_of_the_string() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_strcharpart(&[string("a一b".as_bytes()), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some("一b".as_bytes().to_vec())));
    }

    #[test]
    fn strcharpart_clamps_a_negative_start() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_strcharpart(&[string(b"hello"), num(-2), num(5)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hel".to_vec())));
    }

    // --- f_getpid ---

    #[test]
    fn getpid_is_a_positive_number() {
        let mut rettv = TypvalT::default();
        f_getpid(&[], &mut rettv);
        let TypvalValue::Number(n) = rettv.value else { panic!("expected a Number") };
        assert!(n > 0);
    }

    // --- f_tr ---

    #[test]
    fn tr_matches_the_official_docs_example() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_tr(&[string(b"hello there"), string(b"ht"), string(b"HT")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"Hello THere".to_vec())));
    }

    #[test]
    fn tr_keeps_unmatched_characters_as_is() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_tr(&[string(b"abc"), string(b"xyz"), string(b"XYZ")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"abc".to_vec())));
    }

    #[test]
    fn tr_detects_a_real_length_mismatch() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        // "hello" contains no 'a'/'b', so the first character ('h')
        // triggers the one-time fromstr/tostr length check: fromstr
        // "ab" has 2 characters, tostr "c" has 1 - a real mismatch.
        unsafe { f_tr(&[string(b"hello"), string(b"ab"), string(b"c")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    #[test]
    fn tr_does_not_detect_a_mismatch_when_every_character_matches() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        // A faithfully-preserved original quirk: fromstr "ab" (2
        // chars) vs tostr "x" (1 char) technically mismatch, but
        // since "aa" only ever matches 'a' (never hits the
        // first-unmatched-character check), the mismatch is NEVER
        // detected - both 'a's map to tostr's own first (and only)
        // character.
        unsafe { f_tr(&[string(b"aa"), string(b"ab"), string(b"x")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"xx".to_vec())));
    }

    #[test]
    fn tr_translates_a_multibyte_character() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe {
            f_tr(&[string("a一b".as_bytes()), string("一".as_bytes()), string(b"X")], &mut rettv);
        }
        assert_eq!(rettv.value, TypvalValue::String(Some(b"aXb".to_vec())));
    }

    #[test]
    fn tr_empty_input_is_an_empty_string() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_tr(&[string(b""), string(b"ab"), string(b"xy")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"".to_vec())));
    }

    // --- f_hostname ---

    #[test]
    fn hostname_is_a_nonempty_string() {
        let mut rettv = TypvalT::default();
        f_hostname(&[], &mut rettv);
        let TypvalValue::String(Some(s)) = rettv.value else { panic!("expected a String") };
        assert!(!s.is_empty());
    }

    // --- f_foreground ---

    #[test]
    fn foreground_is_a_no_op() {
        let mut rettv = TypvalT::default();
        f_foreground(&[], &mut rettv);
        // Matches the original's own empty function body: rettv is
        // left completely untouched (still its Default value).
        assert_eq!(rettv.value, TypvalValue::default());
    }

    // --- f_eventhandler ---

    #[test]
    fn eventhandler_reflects_vgetc_busy() {
        let _guard = crate::globals::global_state_test_lock();
        let saved = unsafe { crate::globals::GLOBALS.get_mut() }.vgetc_busy;

        unsafe { crate::globals::GLOBALS.get_mut() }.vgetc_busy = 0;
        let mut rettv = TypvalT::default();
        unsafe { f_eventhandler(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));

        unsafe { crate::globals::GLOBALS.get_mut() }.vgetc_busy = 1;
        unsafe { f_eventhandler(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));

        unsafe { crate::globals::GLOBALS.get_mut() }.vgetc_busy = saved;
    }

    // --- f_did_filetype ---

    #[test]
    fn did_filetype_reflects_b_did_filetype() {
        let _guard = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut crate::buffer_defs::BufT;

        let mut rettv = TypvalT::default();
        unsafe { f_did_filetype(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));

        unsafe { (*crate::globals::GLOBALS.get_mut().curbuf).b_did_filetype = true };
        unsafe { f_did_filetype(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = previous;
    }

    // --- f_garbagecollect ---

    #[test]
    fn garbagecollect_sets_want_garbage_collect() {
        let _guard = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.want_garbage_collect = false;
        globals.garbage_collect_at_exit = false;

        let mut rettv = TypvalT::default();
        unsafe { f_garbagecollect(&[], &mut rettv) };
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        assert!(globals.want_garbage_collect);
        assert!(!globals.garbage_collect_at_exit);

        globals.want_garbage_collect = false;
    }

    #[test]
    fn garbagecollect_atexit_true_also_sets_garbage_collect_at_exit() {
        let _guard = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.want_garbage_collect = false;
        globals.garbage_collect_at_exit = false;

        let mut rettv = TypvalT::default();
        unsafe { f_garbagecollect(&[num(1)], &mut rettv) };
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        assert!(globals.want_garbage_collect);
        assert!(globals.garbage_collect_at_exit);

        globals.want_garbage_collect = false;
        globals.garbage_collect_at_exit = false;
    }

    // --- f_getcharsearch ---

    #[test]
    fn getcharsearch_returns_a_dict_with_the_expected_entries() {
        let _guard = crate::globals::global_state_test_lock();
        crate::search::set_last_csearch(0, b"", 0);
        crate::search::set_last_csearch(i32::from(b'x'), b"x", 1);
        crate::search::set_csearch_direction(crate::vim_defs::Direction::Forward);
        crate::search::set_csearch_until(false);

        let mut rettv = TypvalT::default();
        f_getcharsearch(&[], &mut rettv);
        let TypvalValue::Dict(d) = rettv.value else { panic!("expected a Dict") };
        unsafe {
            let char_item = crate::eval::typval::tv_dict_find(Some(&mut *d), b"char").unwrap();
            assert_eq!(crate::eval::typval::tv_get_string(&(*char_item).di_tv), b"x".to_vec());
            let forward_item = crate::eval::typval::tv_dict_find(Some(&mut *d), b"forward").unwrap();
            assert_eq!(crate::eval::typval::tv_get_number(&(*forward_item).di_tv), 1);
            let until_item = crate::eval::typval::tv_dict_find(Some(&mut *d), b"until").unwrap();
            assert_eq!(crate::eval::typval::tv_get_number(&(*until_item).di_tv), 0);
            crate::eval::typval::tv_dict_unref(d);
        }
    }

    // --- f_mode / non_zero_arg ---

    #[test]
    fn non_zero_arg_missing_is_false() {
        assert!(!non_zero_arg(&[]));
    }

    #[test]
    fn non_zero_arg_nonzero_number_is_true() {
        assert!(non_zero_arg(&[num(1)]));
        assert!(!non_zero_arg(&[num(0)]));
    }

    #[test]
    fn non_zero_arg_nonempty_string_is_true() {
        assert!(non_zero_arg(&[string(b"x")]));
        assert!(!non_zero_arg(&[string(b"")]));
    }

    #[test]
    fn mode_default_reports_only_the_first_character() {
        let mut b = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut b as *mut crate::buffer_defs::BufT);
        unsafe { crate::globals::GLOBALS.get_mut() }.State = crate::state_defs::mode::REPLACE as i32;

        let mut rettv = TypvalT::default();
        unsafe { f_mode(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"R".to_vec())));

        unsafe { crate::globals::GLOBALS.get_mut() }.State = crate::state_defs::mode::NORMAL as i32;
    }

    #[test]
    fn mode_with_nonzero_arg_reports_the_full_string() {
        let mut b = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut b as *mut crate::buffer_defs::BufT);
        unsafe { crate::globals::GLOBALS.get_mut() }.State = crate::state_defs::mode::REPLACE as i32;

        let mut rettv = TypvalT::default();
        unsafe { f_mode(&[num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"R".to_vec())));

        unsafe { crate::globals::GLOBALS.get_mut() }.State = crate::state_defs::mode::VREPLACE as i32;
        unsafe { f_mode(&[num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"Rv".to_vec())));

        unsafe { crate::globals::GLOBALS.get_mut() }.State = crate::state_defs::mode::NORMAL as i32;
    }

    // --- f_visualmode ---

    #[test]
    fn visualmode_reflects_b_visual_mode_eval() {
        let _guard = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { b_visual_mode_eval: i32::from(b'v'), ..Default::default() };
        let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut crate::buffer_defs::BufT;

        let mut rettv = TypvalT::default();
        unsafe { f_visualmode(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"v".to_vec())));
        // Not reset without a nonzero argument.
        assert_eq!(unsafe { (*crate::globals::GLOBALS.get_mut().curbuf).b_visual_mode_eval }, i32::from(b'v'));

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = previous;
    }

    #[test]
    fn visualmode_empty_before_any_visual_mode_used() {
        let _guard = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut crate::buffer_defs::BufT;

        let mut rettv = TypvalT::default();
        unsafe { f_visualmode(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"".to_vec())));

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = previous;
    }

    #[test]
    fn visualmode_nonzero_arg_resets_it() {
        let _guard = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { b_visual_mode_eval: i32::from(b'V'), ..Default::default() };
        let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut crate::buffer_defs::BufT;

        let mut rettv = TypvalT::default();
        unsafe { f_visualmode(&[num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"V".to_vec())));
        assert_eq!(unsafe { (*crate::globals::GLOBALS.get_mut().curbuf).b_visual_mode_eval }, 0);

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = previous;
    }

    // --- f_wildmenumode ---

    #[test]
    fn wildmenumode_reflects_wild_menu_showing() {
        let _guard = crate::globals::global_state_test_lock();
        let saved = unsafe { crate::globals::GLOBALS.get_mut() }.wild_menu_showing;

        unsafe { crate::globals::GLOBALS.get_mut() }.wild_menu_showing = 0;
        let mut rettv = TypvalT::default();
        unsafe { f_wildmenumode(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::default());

        unsafe { crate::globals::GLOBALS.get_mut() }.wild_menu_showing = 1;
        let mut rettv = TypvalT::default();
        unsafe { f_wildmenumode(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));

        unsafe { crate::globals::GLOBALS.get_mut() }.wild_menu_showing = saved;
    }

    // --- f_windowsversion ---

    #[test]
    fn windowsversion_reads_the_null_terminated_buffer() {
        let _guard = crate::globals::global_state_test_lock();
        let saved = unsafe { crate::globals::GLOBALS.get_mut() }.windowsVersion;

        let mut version = [0u8; 20];
        version[..3].copy_from_slice(b"10\0");
        unsafe { crate::globals::GLOBALS.get_mut() }.windowsVersion = version;
        let mut rettv = TypvalT::default();
        f_windowsversion(&[], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::String(Some(b"10".to_vec())));

        unsafe { crate::globals::GLOBALS.get_mut() }.windowsVersion = saved;
    }

    #[test]
    fn windowsversion_is_empty_by_default() {
        let _guard = crate::globals::global_state_test_lock();
        let saved = unsafe { crate::globals::GLOBALS.get_mut() }.windowsVersion;
        unsafe { crate::globals::GLOBALS.get_mut() }.windowsVersion = [0; 20];

        let mut rettv = TypvalT::default();
        f_windowsversion(&[], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::String(Some(b"".to_vec())));

        unsafe { crate::globals::GLOBALS.get_mut() }.windowsVersion = saved;
    }

    // --- f_getreg / getreg_get_regname ---

    #[test]
    fn getreg_black_hole_register_is_an_empty_string() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_getreg(&[string(b"_")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"".to_vec())));
    }

    #[test]
    fn getreg_never_yanked_named_register_is_null_string() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_getreg(&[string(b"a")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    #[test]
    fn getreg_defaults_to_v_register() {
        let _guard = crate::globals::global_state_test_lock();
        unsafe { crate::eval::vars::set_vim_var_string(crate::eval::vars::VimVarIndex::Reg, Some(b"_")) };
        let mut rettv = TypvalT::default();
        unsafe { f_getreg(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"".to_vec())));
        unsafe { crate::eval::vars::set_vim_var_string(crate::eval::vars::VimVarIndex::Reg, Some(b"\"")) };
    }

    #[test]
    fn getreg_empty_regname_falls_back_to_unnamed() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { getreg_get_regname(&[string(b"")]) }, i32::from(b'"'));
    }

    #[test]
    fn getreg_type_error_leaves_rettv_untouched() {
        let mut rettv = TypvalT::default();
        let list_tv = TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() };
        unsafe { f_getreg(&[list_tv], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::default());
    }

    #[test]
    #[should_panic(expected = "kGRegList")]
    fn getreg_list_true_is_unimplemented() {
        let _guard = crate::globals::global_state_test_lock();
        // Uses an ordinary named register ('a'), not the black hole
        // ('_') or another special register - those are resolved by
        // get_spec_reg BEFORE get_reg_contents ever reaches its own
        // kGRegList check, so they never trigger this panic (caught
        // via a real test failure, not assumed).
        let mut rettv = TypvalT::default();
        unsafe { f_getreg(&[string(b"a"), num(0), num(1)], &mut rettv) };
    }
}