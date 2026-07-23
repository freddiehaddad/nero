//! Translated from `src/nvim/option_defs.h` (partial).
//!
//! Translated: `OptFlags`, `OptValType`+`OptValData` (unified into one
//! safe `OptVal` enum - see its doc comment), `OptScope`(+`OptScopeFlags`),
//! `set_op_T`; and, from the machine-generated
//! `options_enum.generated.h` (spliced into this header in the
//! original): `OptIndex`/`GlobalOptIndex`/`BufOptIndex`/`WinOptIndex`/
//! `TabOptIndex` (the ~800 total enum variants, mechanically transcribed
//! via a throwaway parser script rather than hand-typed - see
//! `OptIndex`'s own doc comment) and the `GLOBAL_OPT_IDX`/`BUF_OPT_IDX`/
//! `WIN_OPT_IDX`/`TAB_OPT_IDX` lookup tables that map each local index
//! back to its `OptIndex`.
//!
//! Now also translated, now that `eval/typval_defs.rs`'s `SctxT` is
//! real: `optset_T`/`opt_did_set_cb_T` (as [`OptsetT`]/`OptDidSetCbT`)
//! and `optexpand_T`/`opt_expand_cb_T` (as [`OptexpandT`]/
//! `OptExpandCbT`, using the already-existing opaque `RegmatchT` for
//! `oe_regmatch` and a NEW opaque `ExpandT` placeholder for `oe_xp` -
//! `cmdexpand_defs.h`'s real `expand_T`, itself a substantial cmdline-
//! completion type not otherwise needed yet), and `vimoption_T`
//! itself (as [`VimoptionT`]) - its `type: OptValType` field needed a
//! small standalone [`OptValType`] enum reintroduced alongside `OptVal`
//! (which unified the original's *tag+value* pair for actual option
//! *values*, but `vimoption_T.type` is a bare tag describing what kind
//! of value an option accepts, with no value attached - `OptVal` itself
//! doesn't fit that use case).
//!
//! Deferred: the actual, populated `options[]` table itself (needs the
//! machine-generated `options.generated.h`'s ~8000-line table content,
//! each entry's `var`/`flags_var` pointing at a real
//! `option_vars.rs`/`buffer_defs.rs`/`globals.rs` field - a real
//! per-option address-resolution design question of its own - plus
//! real `opt_did_set_cb`/`opt_expand_cb` callback functions, none of
//! which exist yet) - a substantial, separate undertaking of its own,
//! not started. `VimoptionT`'s own field *shape* needed none of that,
//! matching the `OptIndex`-before-`options[]`-engine split already
//! established this session for `ex_cmds_defs.rs`'s `CommandDefinition`.

use crate::api::private::defs::NvimString;
use crate::eval::typval_defs::SctxT;
use crate::types_defs::{OptInt, RegmatchT, TriState};

/// Option flags (`OptFlags`). Kept as plain `u32` bit-flag constants (some
/// of which are themselves combinations of others, e.g. `REDR_ALL`), not a
/// Rust `enum`, since they combine via bitwise OR - not mutually exclusive
/// variants.
pub mod opt_flags {
    /// Environment expansion. NOTE: can never be used for local or hidden
    /// options.
    pub const EXPAND: u32 = 1 << 0;
    /// Don't expand default value.
    pub const NO_DEF_EXP: u32 = 1 << 1;
    /// Don't set to default value.
    pub const NO_DEFAULT: u32 = 1 << 2;
    /// Option has been set/reset.
    pub const WAS_SET: u32 = 1 << 3;
    /// Don't include in `:mkvimrc` output.
    pub const NO_MKRC: u32 = 1 << 4;
    /// Send option to remote UI.
    pub const UI_OPTION: u32 = 1 << 5;
    /// Redraw tabline.
    pub const REDR_TABL: u32 = 1 << 6;
    /// Redraw status lines.
    pub const REDR_STAT: u32 = 1 << 7;
    /// Redraw current window and recompute text.
    pub const REDR_WIN: u32 = 1 << 8;
    /// Redraw current buffer and recompute text.
    pub const REDR_BUF: u32 = 1 << 9;
    /// Redraw all windows and recompute text.
    pub const REDR_ALL: u32 = REDR_BUF | REDR_WIN;
    /// Clear and redraw all and recompute text.
    pub const REDR_CLEAR: u32 = REDR_ALL | REDR_STAT;
    /// Comma-separated list.
    pub const COMMA: u32 = 1 << 10;
    /// Comma-separated list that cannot have two consecutive commas.
    pub const ONE_COMMA: u32 = (1 << 11) | COMMA;
    /// Don't allow duplicate strings.
    pub const NO_DUP: u32 = 1 << 12;
    /// List of single-char flags.
    pub const FLAG_LIST: u32 = 1 << 13;
    /// Cannot change in modeline or secure mode.
    pub const SECURE: u32 = 1 << 14;
    /// Expand default value with `_()`.
    pub const GETTEXT: u32 = 1 << 15;
    /// Do not use local value for global vimrc.
    pub const NO_GLOB: u32 = 1 << 16;
    /// Only normal file name chars allowed.
    pub const NFNAME: u32 = 1 << 17;
    /// Option was set from a modeline.
    pub const INSECURE: u32 = 1 << 18;
    /// Priority for `:mkvimrc` (setting option has side effects).
    pub const PRI_MKRC: u32 = 1 << 19;
    /// Update curswant required; not needed when there is a redraw flag.
    pub const CURSWANT: u32 = 1 << 20;
    /// Only normal directory name chars allowed.
    pub const NDNAME: u32 = 1 << 21;
    /// Option only changes highlight, not text.
    pub const HL_ONLY: u32 = 1 << 22;
    /// Under control of `'modelineexpr'`.
    pub const MLE: u32 = 1 << 23;
    /// Accept a function reference or a lambda.
    pub const FUNC: u32 = 1 << 24;
    /// Values use colons to create sublists.
    pub const COLON: u32 = 1 << 25;
}

/// Option value type/value (`OptValType`+`OptValData`, unified into one
/// safe Rust enum): the original stores these as two separate fields
/// (`OptVal { OptValType type; OptValData data; }`, the latter a union of
/// `TriState`/`OptInt`/`String`) - but since the tag (`type`) and the data
/// always live right next to each other in the same `OptVal` struct
/// (unlike `DecorInlineData`/`MtKey` elsewhere in this crate, where the tag
/// lives in a *different*, external struct for compact inline storage),
/// there is no memory-layout reason to keep them as an untagged union
/// here - a safe tagged enum is a direct, lossless simplification.
///
/// Boolean options are actually tri-states because they have a third
/// "None" value (kept from the original's comment on `OptValData.boolean`).
#[derive(Debug, Clone, PartialEq)]
pub enum OptVal {
    /// Make sure Nil can't be bitshifted and used as an option type flag
    /// (kept from the original's comment on `kOptValTypeNil = -1`; not
    /// meaningful as a bit position in this translation, but the ordering/
    /// semantics are preserved).
    Nil,
    Boolean(TriState),
    Number(OptInt),
    String(NvimString),
}

/// Bare *type tag* for an option's accepted value kind, with no value
/// attached (`OptValType`) - reintroduced standalone alongside
/// [`OptVal`] specifically for [`VimoptionT`]'s own `type` field: an
/// entry in the (still-deferred) `options[]` table describes what
/// *kind* of value a given option accepts, independent of any
/// particular value, so `OptVal`'s own tag-plus-value unification
/// doesn't fit this use site the way it does for actual runtime option
/// values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptValType {
    /// (`kOptValTypeNil = -1`).
    Nil = -1,
    /// (`kOptValTypeBoolean`).
    Boolean = 0,
    /// (`kOptValTypeNumber`).
    Number = 1,
    /// (`kOptValTypeString`).
    String = 2,
}

/// Scopes that an option can support (`OptScope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OptScope {
    /// Request global option value.
    Global = 0,
    /// Request window-local option value.
    Win,
    /// Request buffer-local option value.
    Buf,
    /// Request tabpage-local option value.
    Tab,
}

/// Always update this whenever a new option scope is added (`kOptScopeSize`).
pub const OPT_SCOPE_SIZE: usize = OptScope::Tab as usize + 1;

pub type OptScopeFlags = u8;

/// `:set` operator types (`set_op_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetOpT {
    None = 0,
    /// `"opt+=arg"`
    Adding,
    /// `"opt^=arg"`
    Prepending,
    /// `"opt-=arg"`
    Removing,
}

/// Argument for the callback function ([`OptDidSetCbT`]) invoked after
/// an option value is modified (`optset_T`).
pub struct OptsetT {
    /// Pointer to the option variable. The variable can be an `OptInt`
    /// (numeric option), an `int` (boolean option) or a char pointer
    /// (string option) in the original - kept as an untyped raw
    /// pointer here too, matching the original's own `void *`
    /// type-erasure exactly rather than inventing a more specific type
    /// the header itself doesn't have (`os_varp`).
    pub os_varp: *mut std::ffi::c_void,
    pub os_idx: OptIndex,
    pub os_flags: i32,
    /// Old value of the option. Self-tagged (see [`OptVal`]'s own doc
    /// comment) unlike the original's untagged `OptValData os_oldval`
    /// (`os_oldval`).
    pub os_oldval: OptVal,
    /// New value of the option (`os_newval`).
    pub os_newval: OptVal,
    /// Option value was checked to be safe, no need to set
    /// `kOptFlagInsecure`. Used for the `'keymap'`, `'filetype'` and
    /// `'syntax'` options (`os_value_checked`).
    pub os_value_checked: bool,
    /// Option value changed. Used for the `'filetype'` and `'syntax'`
    /// options (`os_value_changed`).
    pub os_value_changed: bool,
    /// Used by the `'isident'`, `'iskeyword'`, `'isprint'` and
    /// `'isfname'` options: `true` if the character table was
    /// modified while processing the option and needs to be restored
    /// because of a failure (`os_restore_chartab`).
    pub os_restore_chartab: bool,
    /// If the value specified for an option is not valid and the
    /// error message is parameterized, this buffer holds the error
    /// message (`os_errbuf`).
    pub os_errbuf: Option<Vec<u8>>,
    /// length of the error buffer (`os_errbuflen`).
    pub os_errbuflen: usize,
    /// `*mut WinT`, untyped to match the original's own `void *`
    /// (`os_win`).
    pub os_win: *mut std::ffi::c_void,
    /// `*mut BufT`, untyped to match the original's own `void *`
    /// (`os_buf`).
    pub os_buf: *mut std::ffi::c_void,
}

impl Default for OptsetT {
    fn default() -> Self {
        OptsetT {
            os_varp: std::ptr::null_mut(),
            os_idx: OptIndex::Invalid,
            os_flags: 0,
            os_oldval: OptVal::Nil,
            os_newval: OptVal::Nil,
            os_value_checked: false,
            os_value_changed: false,
            os_restore_chartab: false,
            os_errbuf: None,
            os_errbuflen: 0,
            os_win: std::ptr::null_mut(),
            os_buf: std::ptr::null_mut(),
        }
    }
}

/// Type for the callback function that is invoked after an option
/// value is changed to validate and apply the new value
/// (`opt_did_set_cb_T`).
///
/// Returns `None` if the option value is valid and successfully
/// applied. Otherwise returns an error message.
pub type OptDidSetCbT = fn(args: &mut OptsetT) -> Option<&'static [u8]>;

/// Argument for the callback function ([`OptExpandCbT`]) invoked after
/// a string option value is expanded for cmdline completion
/// (`optexpand_T`).
pub struct OptexpandT {
    /// Pointer to the option variable. It's always a string in the
    /// original - kept as an untyped raw pointer here too, matching
    /// `OptsetT.os_varp`'s own reasoning (`oe_varp`).
    pub oe_varp: *mut std::ffi::c_void,
    pub oe_idx: OptIndex,
    /// The original option value, escaped (`oe_opt_value`).
    pub oe_opt_value: Option<Vec<u8>>,
    /// `true` if using `set+=` instead of `set=` (`oe_append`).
    pub oe_append: bool,
    /// `true` if we would like to add the original option value as
    /// the first choice (`oe_include_orig_val`).
    pub oe_include_orig_val: bool,
    /// Regex from the cmdline, for matching potential options against
    /// (`oe_regmatch`).
    pub oe_regmatch: *mut RegmatchT,
    /// The expansion context (`oe_xp`).
    pub oe_xp: *mut crate::types_defs::ExpandT,
    /// The full argument passed to `:set`. For example, if the user
    /// inputs `":set dip=icase,algorithm:my<Tab>"`, `oe_xp`'s own
    /// pattern will only have `"my"`, but this will contain the whole
    /// `"icase,algorithm:my"` (`oe_set_arg`).
    pub oe_set_arg: Option<Vec<u8>>,
}

impl Default for OptexpandT {
    fn default() -> Self {
        OptexpandT {
            oe_varp: std::ptr::null_mut(),
            oe_idx: OptIndex::Invalid,
            oe_opt_value: None,
            oe_append: false,
            oe_include_orig_val: false,
            oe_regmatch: std::ptr::null_mut(),
            oe_xp: std::ptr::null_mut(),
            oe_set_arg: None,
        }
    }
}

/// Type for the callback function that is invoked when expanding
/// possible string option values during cmdline completion
/// (`opt_expand_cb_T`).
///
/// Returns `Some(matches)` if the expansion succeeded, `None`
/// otherwise (collapsing the original's separate `int` return code +
/// `numMatches`/`matches` out-parameters into one `Option`, matching
/// this crate's usual preference for a single meaningful return value
/// over a C-style status-code-plus-out-parameters pair).
pub type OptExpandCbT = fn(args: &mut OptexpandT) -> Option<Vec<Vec<u8>>>;

/// Structure for one entry of the (still-deferred, see this module's
/// own doc comment) `options[]` table (`vimoption_T`).
pub struct VimoptionT {
    /// full option name (`fullname`).
    pub fullname: &'static [u8],
    /// permissible abbreviation (`shortname`).
    pub shortname: &'static [u8],
    /// see [`opt_flags`] (`flags`).
    pub flags: u32,
    /// option type (`type`).
    pub r#type: OptValType,
    /// option scope flags, see [`OptScope`] (`scope_flags`).
    pub scope_flags: OptScopeFlags,
    /// global option: pointer to variable; window-local option: null;
    /// buffer-local option: global value. Untyped raw pointer,
    /// matching the original's own `void *` (`var`).
    pub var: *mut std::ffi::c_void,
    pub flags_var: *mut u32,
    /// index of option at every scope (`scope_idx`).
    pub scope_idx: [isize; OPT_SCOPE_SIZE],
    /// option is immutable, trying to set it will give an error
    /// (`immutable`).
    pub immutable: bool,
    /// possible values for string options (`values`).
    pub values: &'static [&'static [u8]],
    /// callback function to invoke after an option is modified, to
    /// validate and apply the new value (`opt_did_set_cb`).
    pub opt_did_set_cb: Option<OptDidSetCbT>,
    /// callback function to invoke when expanding possible values on
    /// the cmdline; only useful for string options (`opt_expand_cb`).
    pub opt_expand_cb: Option<OptExpandCbT>,
    /// default value (`def_val`).
    pub def_val: OptVal,
    /// script in which the option was last set (`script_ctx`).
    pub script_ctx: SctxT,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_flags_combinations_match_c_macros() {
        assert_eq!(opt_flags::REDR_ALL, opt_flags::REDR_BUF | opt_flags::REDR_WIN);
        assert_eq!(opt_flags::REDR_CLEAR, opt_flags::REDR_ALL | opt_flags::REDR_STAT);
        assert_eq!(opt_flags::ONE_COMMA, (1 << 11) | opt_flags::COMMA);
    }

    #[test]
    fn opt_scope_size_matches_c_macro() {
        assert_eq!(OPT_SCOPE_SIZE, 4); // Global, Win, Buf, Tab
    }

    #[test]
    fn opt_val_variants_are_distinguishable() {
        assert_ne!(OptVal::Nil, OptVal::Number(0));
        assert_eq!(OptVal::Number(5), OptVal::Number(5));
        assert_ne!(OptVal::Boolean(TriState::True), OptVal::Boolean(TriState::False));
    }

    #[test]
    fn opt_val_type_discriminants_match_c_enum() {
        assert_eq!(OptValType::Nil as i32, -1);
        assert_eq!(OptValType::Boolean as i32, 0);
        assert_eq!(OptValType::Number as i32, 1);
        assert_eq!(OptValType::String as i32, 2);
    }

    #[test]
    fn optset_default_is_zeroed_with_nil_values_and_null_pointers() {
        let os = OptsetT::default();
        assert!(os.os_varp.is_null());
        assert_eq!(os.os_idx, OptIndex::Invalid);
        assert_eq!(os.os_flags, 0);
        assert_eq!(os.os_oldval, OptVal::Nil);
        assert_eq!(os.os_newval, OptVal::Nil);
        assert!(!os.os_value_checked);
        assert!(!os.os_value_changed);
        assert!(!os.os_restore_chartab);
        assert!(os.os_errbuf.is_none());
        assert_eq!(os.os_errbuflen, 0);
        assert!(os.os_win.is_null());
        assert!(os.os_buf.is_null());
    }

    #[test]
    fn optexpand_default_is_zeroed_with_null_pointers() {
        let oe = OptexpandT::default();
        assert!(oe.oe_varp.is_null());
        assert_eq!(oe.oe_idx, OptIndex::Invalid);
        assert!(oe.oe_opt_value.is_none());
        assert!(!oe.oe_append);
        assert!(!oe.oe_include_orig_val);
        assert!(oe.oe_regmatch.is_null());
        assert!(oe.oe_xp.is_null());
        assert!(oe.oe_set_arg.is_none());
    }

    #[test]
    fn opt_did_set_cb_t_can_be_stored_and_called() {
        fn validator(_args: &mut OptsetT) -> Option<&'static [u8]> {
            Some(b"E123: bad value")
        }
        let cb: OptDidSetCbT = validator;
        let mut args = OptsetT::default();
        assert_eq!(cb(&mut args), Some(b"E123: bad value".as_slice()));
    }

    #[test]
    fn opt_expand_cb_t_can_be_stored_and_called() {
        fn expander(_args: &mut OptexpandT) -> Option<Vec<Vec<u8>>> {
            Some(vec![b"foo".to_vec(), b"bar".to_vec()])
        }
        let cb: OptExpandCbT = expander;
        let mut args = OptexpandT::default();
        assert_eq!(cb(&mut args), Some(vec![b"foo".to_vec(), b"bar".to_vec()]));
    }

    #[test]
    fn vimoption_t_can_be_constructed_with_no_callbacks() {
        let opt = VimoptionT {
            fullname: b"aleph",
            shortname: b"al",
            flags: 0,
            r#type: OptValType::Number,
            scope_flags: 1 << (OptScope::Global as u8),
            var: std::ptr::null_mut(),
            flags_var: std::ptr::null_mut(),
            scope_idx: [0; OPT_SCOPE_SIZE],
            immutable: true,
            values: &[],
            opt_did_set_cb: None,
            opt_expand_cb: None,
            def_val: OptVal::Number(224),
            script_ctx: SctxT::default(),
        };
        assert_eq!(opt.fullname, b"aleph");
        assert_eq!(opt.def_val, OptVal::Number(224));
        assert!(opt.opt_did_set_cb.is_none());
    }
}

/// Enumerates every option by index (`OptIndex`), from the
/// machine-generated `options_enum.generated.h` (produced by
/// `src/gen/gen_options.lua` from `src/options.lua`'s master option
/// table) - translated directly from a pre-built copy of that
/// generated file found in this checkout's own
/// `build/src/nvim/auto/options_enum.generated.h`, matching this
/// project's "translate the generator's output directly" convention
/// (same as `option_vars.rs`'s own generated content). Mechanically
/// transcribed via a throwaway parser script (not hand-typed, to avoid
/// transcription errors across ~800 enum variants) and independently
/// cross-checked: variant count, first/last entries, and strict
/// 0..N-1 sequential-value ordering all verified against a second,
/// separately-written extraction pass before trusting the result.
///
/// `#[repr(i32)]` with explicit discriminants on every variant,
/// matching the original's exact numeric values (needed since the
/// `*_OPT_IDX` lookup tables below, and the eventual `options[]` table
/// itself, are conceptually indexed by these exact numbers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum OptIndex {
    Invalid = -1,
    Aleph = 0,
    Allowrevins = 1,
    Ambiwidth = 2,
    Arabic = 3,
    Arabicshape = 4,
    Autochdir = 5,
    Autocomplete = 6,
    Autocompletedelay = 7,
    Autocompletetimeout = 8,
    Autoindent = 9,
    Autoread = 10,
    Autowrite = 11,
    Autowriteall = 12,
    Background = 13,
    Backspace = 14,
    Backup = 15,
    Backupcopy = 16,
    Backupdir = 17,
    Backupext = 18,
    Backupskip = 19,
    Belloff = 20,
    Binary = 21,
    Bomb = 22,
    Breakat = 23,
    Breakindent = 24,
    Breakindentopt = 25,
    Browsedir = 26,
    Bufhidden = 27,
    Buflisted = 28,
    Buftype = 29,
    Busy = 30,
    Casemap = 31,
    Cdhome = 32,
    Cdpath = 33,
    Cedit = 34,
    Channel = 35,
    Charconvert = 36,
    Chistory = 37,
    Cindent = 38,
    Cinkeys = 39,
    Cinoptions = 40,
    Cinscopedecls = 41,
    Cinwords = 42,
    Clipboard = 43,
    Cmdheight = 44,
    Cmdwinheight = 45,
    Colorcolumn = 46,
    Columns = 47,
    Comments = 48,
    Commentstring = 49,
    Compatible = 50,
    Complete = 51,
    Completefunc = 52,
    Completeitemalign = 53,
    Completeopt = 54,
    Completeslash = 55,
    Completetimeout = 56,
    Concealcursor = 57,
    Conceallevel = 58,
    Confirm = 59,
    Copyindent = 60,
    Cpoptions = 61,
    Cursorbind = 62,
    Cursorcolumn = 63,
    Cursorline = 64,
    Cursorlineopt = 65,
    Debug = 66,
    Define = 67,
    Delcombine = 68,
    Dictionary = 69,
    Diff = 70,
    Diffanchors = 71,
    Diffexpr = 72,
    Diffopt = 73,
    Digraph = 74,
    Directory = 75,
    Display = 76,
    Eadirection = 77,
    Edcompatible = 78,
    Emoji = 79,
    Encoding = 80,
    Endoffile = 81,
    Endofline = 82,
    Equalalways = 83,
    Equalprg = 84,
    Errorbells = 85,
    Errorfile = 86,
    Errorformat = 87,
    Eventignore = 88,
    Eventignorewin = 89,
    Expandtab = 90,
    Exrc = 91,
    Fileencoding = 92,
    Fileencodings = 93,
    Fileformat = 94,
    Fileformats = 95,
    Fileignorecase = 96,
    Filetype = 97,
    Fillchars = 98,
    Findfunc = 99,
    Fixendofline = 100,
    Foldclose = 101,
    Foldcolumn = 102,
    Foldenable = 103,
    Foldexpr = 104,
    Foldignore = 105,
    Foldlevel = 106,
    Foldlevelstart = 107,
    Foldmarker = 108,
    Foldmethod = 109,
    Foldminlines = 110,
    Foldnestmax = 111,
    Foldopen = 112,
    Foldtext = 113,
    Formatexpr = 114,
    Formatlistpat = 115,
    Formatoptions = 116,
    Formatprg = 117,
    Fsync = 118,
    Gdefault = 119,
    Grepformat = 120,
    Grepprg = 121,
    Guicursor = 122,
    Guifont = 123,
    Guifontwide = 124,
    Guioptions = 125,
    Guitablabel = 126,
    Guitabtooltip = 127,
    Helpfile = 128,
    Helpheight = 129,
    Helplang = 130,
    Hidden = 131,
    Highlight = 132,
    History = 133,
    Hkmap = 134,
    Hkmapp = 135,
    Hlsearch = 136,
    Icon = 137,
    Iconstring = 138,
    Ignorecase = 139,
    Imcmdline = 140,
    Imdisable = 141,
    Iminsert = 142,
    Imsearch = 143,
    Inccommand = 144,
    Include = 145,
    Includeexpr = 146,
    Incsearch = 147,
    Indentexpr = 148,
    Indentkeys = 149,
    Infercase = 150,
    Insertmode = 151,
    Isfname = 152,
    Isident = 153,
    Iskeyword = 154,
    Isprint = 155,
    Joinspaces = 156,
    Jumpoptions = 157,
    Keymap = 158,
    Keymodel = 159,
    Keywordprg = 160,
    Langmap = 161,
    Langmenu = 162,
    Langnoremap = 163,
    Langremap = 164,
    Laststatus = 165,
    Lazyredraw = 166,
    Lhistory = 167,
    Linebreak = 168,
    Lines = 169,
    Linespace = 170,
    Lisp = 171,
    Lispoptions = 172,
    Lispwords = 173,
    List = 174,
    Listchars = 175,
    Loadplugins = 176,
    Magic = 177,
    Makeef = 178,
    Makeencoding = 179,
    Makeprg = 180,
    Matchpairs = 181,
    Matchtime = 182,
    Maxcombine = 183,
    Maxfuncdepth = 184,
    Maxmapdepth = 185,
    Maxmempattern = 186,
    Maxsearchcount = 187,
    Menuitems = 188,
    Messagesopt = 189,
    Mkspellmem = 190,
    Modeline = 191,
    Modelineexpr = 192,
    Modelines = 193,
    Modifiable = 194,
    Modified = 195,
    More = 196,
    Mouse = 197,
    Mousefocus = 198,
    Mousehide = 199,
    Mousemodel = 200,
    Mousemoveevent = 201,
    Mousescroll = 202,
    Mouseshape = 203,
    Mousetime = 204,
    Nrformats = 205,
    Number = 206,
    Numberwidth = 207,
    Omnifunc = 208,
    Opendevice = 209,
    Operatorfunc = 210,
    Packlockfile = 211,
    Packpath = 212,
    Paragraphs = 213,
    Paste = 214,
    Pastetoggle = 215,
    Patchexpr = 216,
    Patchmode = 217,
    Path = 218,
    Preserveindent = 219,
    Previewheight = 220,
    Previewwindow = 221,
    Prompt = 222,
    Pumblend = 223,
    Pumborder = 224,
    Pumheight = 225,
    Pummaxwidth = 226,
    Pumwidth = 227,
    Pyxversion = 228,
    Quickfixtextfunc = 229,
    Quoteescape = 230,
    Readonly = 231,
    Redrawdebug = 232,
    Redrawtime = 233,
    Regexpengine = 234,
    Relativenumber = 235,
    Remap = 236,
    Report = 237,
    Revins = 238,
    Rightleft = 239,
    Rightleftcmd = 240,
    Ruler = 241,
    Rulerformat = 242,
    Runtimepath = 243,
    Scroll = 244,
    Scrollback = 245,
    Scrollbind = 246,
    Scrolljump = 247,
    Scrolloff = 248,
    Scrolloffpad = 249,
    Scrollopt = 250,
    Sections = 251,
    Secure = 252,
    Selection = 253,
    Selectmode = 254,
    Sessionoptions = 255,
    Shada = 256,
    Shadafile = 257,
    Shell = 258,
    Shellcmdflag = 259,
    Shellpipe = 260,
    Shellquote = 261,
    Shellredir = 262,
    Shellslash = 263,
    Shelltemp = 264,
    Shellxescape = 265,
    Shellxquote = 266,
    Shiftround = 267,
    Shiftwidth = 268,
    Shortmess = 269,
    Showbreak = 270,
    Showcmd = 271,
    Showcmdloc = 272,
    Showfulltag = 273,
    Showmatch = 274,
    Showmode = 275,
    Showtabline = 276,
    Sidescroll = 277,
    Sidescrolloff = 278,
    Signcolumn = 279,
    Smartcase = 280,
    Smartindent = 281,
    Smarttab = 282,
    Smoothscroll = 283,
    Softtabstop = 284,
    Spell = 285,
    Spellcapcheck = 286,
    Spellfile = 287,
    Spelllang = 288,
    Spelloptions = 289,
    Spellsuggest = 290,
    Splitbelow = 291,
    Splitkeep = 292,
    Splitright = 293,
    Startofline = 294,
    Statuscolumn = 295,
    Statusline = 296,
    Suffixes = 297,
    Suffixesadd = 298,
    Swapfile = 299,
    Switchbuf = 300,
    Synmaxcol = 301,
    Syntax = 302,
    Tabclose = 303,
    Tabline = 304,
    Tabpagemax = 305,
    Tabstop = 306,
    Tagbsearch = 307,
    Tagcase = 308,
    Tagfunc = 309,
    Taglength = 310,
    Tagrelative = 311,
    Tags = 312,
    Tagstack = 313,
    Termbidi = 314,
    Termencoding = 315,
    Termguicolors = 316,
    Termpastefilter = 317,
    Termsync = 318,
    Terse = 319,
    Textwidth = 320,
    Thesaurus = 321,
    Thesaurusfunc = 322,
    Tildeop = 323,
    Timeout = 324,
    Timeoutlen = 325,
    Title = 326,
    Titlelen = 327,
    Titleold = 328,
    Titlestring = 329,
    Ttimeout = 330,
    Ttimeoutlen = 331,
    Ttyfast = 332,
    Undodir = 333,
    Undofile = 334,
    Undolevels = 335,
    Undoreload = 336,
    Updatecount = 337,
    Updatetime = 338,
    Varsofttabstop = 339,
    Vartabstop = 340,
    Verbose = 341,
    Verbosefile = 342,
    Viewdir = 343,
    Viewoptions = 344,
    Virtualedit = 345,
    Visualbell = 346,
    Warn = 347,
    Whichwrap = 348,
    Wildchar = 349,
    Wildcharm = 350,
    Wildignore = 351,
    Wildignorecase = 352,
    Wildmenu = 353,
    Wildmode = 354,
    Wildoptions = 355,
    Winaltkeys = 356,
    Winbar = 357,
    Winblend = 358,
    Winborder = 359,
    Window = 360,
    Winfixbuf = 361,
    Winfixheight = 362,
    Winfixwidth = 363,
    Winheight = 364,
    Winhighlight = 365,
    Winminheight = 366,
    Winminwidth = 367,
    Winpinned = 368,
    Winwidth = 369,
    Wrap = 370,
    Wrapmargin = 371,
    Wrapscan = 372,
    Write = 373,
    Writeany = 374,
    Writebackup = 375,
    Writedelay = 376,
}

/// Always update alongside [`OptIndex`] (`kOptCount`).
pub const OPT_COUNT: usize = 377;

/// Subset of [`OptIndex`] for options that have a global value
/// (`GlobalOptIndex`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum GlobalOptIndex {
    Invalid = -1,
    Aleph = 0,
    Allowrevins = 1,
    Ambiwidth = 2,
    Arabicshape = 3,
    Autochdir = 4,
    Autocomplete = 5,
    Autocompletedelay = 6,
    Autocompletetimeout = 7,
    Autoread = 8,
    Autowrite = 9,
    Autowriteall = 10,
    Background = 11,
    Backspace = 12,
    Backup = 13,
    Backupcopy = 14,
    Backupdir = 15,
    Backupext = 16,
    Backupskip = 17,
    Belloff = 18,
    Breakat = 19,
    Browsedir = 20,
    Casemap = 21,
    Cdhome = 22,
    Cdpath = 23,
    Cedit = 24,
    Charconvert = 25,
    Chistory = 26,
    Clipboard = 27,
    Cmdheight = 28,
    Cmdwinheight = 29,
    Columns = 30,
    Compatible = 31,
    Completeitemalign = 32,
    Completeopt = 33,
    Completetimeout = 34,
    Confirm = 35,
    Cpoptions = 36,
    Debug = 37,
    Define = 38,
    Delcombine = 39,
    Dictionary = 40,
    Diffanchors = 41,
    Diffexpr = 42,
    Diffopt = 43,
    Digraph = 44,
    Directory = 45,
    Display = 46,
    Eadirection = 47,
    Edcompatible = 48,
    Emoji = 49,
    Encoding = 50,
    Equalalways = 51,
    Equalprg = 52,
    Errorbells = 53,
    Errorfile = 54,
    Errorformat = 55,
    Eventignore = 56,
    Exrc = 57,
    Fileencodings = 58,
    Fileformats = 59,
    Fileignorecase = 60,
    Fillchars = 61,
    Findfunc = 62,
    Foldclose = 63,
    Foldlevelstart = 64,
    Foldopen = 65,
    Formatprg = 66,
    Fsync = 67,
    Gdefault = 68,
    Grepformat = 69,
    Grepprg = 70,
    Guicursor = 71,
    Guifont = 72,
    Guifontwide = 73,
    Guioptions = 74,
    Guitablabel = 75,
    Guitabtooltip = 76,
    Helpfile = 77,
    Helpheight = 78,
    Helplang = 79,
    Hidden = 80,
    Highlight = 81,
    History = 82,
    Hkmap = 83,
    Hkmapp = 84,
    Hlsearch = 85,
    Icon = 86,
    Iconstring = 87,
    Ignorecase = 88,
    Imcmdline = 89,
    Imdisable = 90,
    Inccommand = 91,
    Include = 92,
    Incsearch = 93,
    Insertmode = 94,
    Isfname = 95,
    Isident = 96,
    Isprint = 97,
    Joinspaces = 98,
    Jumpoptions = 99,
    Keymodel = 100,
    Keywordprg = 101,
    Langmap = 102,
    Langmenu = 103,
    Langnoremap = 104,
    Langremap = 105,
    Laststatus = 106,
    Lazyredraw = 107,
    Lines = 108,
    Linespace = 109,
    Lispwords = 110,
    Listchars = 111,
    Loadplugins = 112,
    Magic = 113,
    Makeef = 114,
    Makeencoding = 115,
    Makeprg = 116,
    Matchtime = 117,
    Maxcombine = 118,
    Maxfuncdepth = 119,
    Maxmapdepth = 120,
    Maxmempattern = 121,
    Maxsearchcount = 122,
    Menuitems = 123,
    Messagesopt = 124,
    Mkspellmem = 125,
    Modelineexpr = 126,
    Modelines = 127,
    More = 128,
    Mouse = 129,
    Mousefocus = 130,
    Mousehide = 131,
    Mousemodel = 132,
    Mousemoveevent = 133,
    Mousescroll = 134,
    Mouseshape = 135,
    Mousetime = 136,
    Opendevice = 137,
    Operatorfunc = 138,
    Packlockfile = 139,
    Packpath = 140,
    Paragraphs = 141,
    Paste = 142,
    Pastetoggle = 143,
    Patchexpr = 144,
    Patchmode = 145,
    Path = 146,
    Previewheight = 147,
    Prompt = 148,
    Pumblend = 149,
    Pumborder = 150,
    Pumheight = 151,
    Pummaxwidth = 152,
    Pumwidth = 153,
    Pyxversion = 154,
    Quickfixtextfunc = 155,
    Redrawdebug = 156,
    Redrawtime = 157,
    Regexpengine = 158,
    Remap = 159,
    Report = 160,
    Revins = 161,
    Ruler = 162,
    Rulerformat = 163,
    Runtimepath = 164,
    Scrolljump = 165,
    Scrolloff = 166,
    Scrolloffpad = 167,
    Scrollopt = 168,
    Sections = 169,
    Secure = 170,
    Selection = 171,
    Selectmode = 172,
    Sessionoptions = 173,
    Shada = 174,
    Shadafile = 175,
    Shell = 176,
    Shellcmdflag = 177,
    Shellpipe = 178,
    Shellquote = 179,
    Shellredir = 180,
    Shellslash = 181,
    Shelltemp = 182,
    Shellxescape = 183,
    Shellxquote = 184,
    Shiftround = 185,
    Shortmess = 186,
    Showbreak = 187,
    Showcmd = 188,
    Showcmdloc = 189,
    Showfulltag = 190,
    Showmatch = 191,
    Showmode = 192,
    Showtabline = 193,
    Sidescroll = 194,
    Sidescrolloff = 195,
    Smartcase = 196,
    Smarttab = 197,
    Spellsuggest = 198,
    Splitbelow = 199,
    Splitkeep = 200,
    Splitright = 201,
    Startofline = 202,
    Statusline = 203,
    Suffixes = 204,
    Switchbuf = 205,
    Tabclose = 206,
    Tabline = 207,
    Tabpagemax = 208,
    Tagbsearch = 209,
    Tagcase = 210,
    Taglength = 211,
    Tagrelative = 212,
    Tags = 213,
    Tagstack = 214,
    Termbidi = 215,
    Termencoding = 216,
    Termguicolors = 217,
    Termpastefilter = 218,
    Termsync = 219,
    Terse = 220,
    Thesaurus = 221,
    Thesaurusfunc = 222,
    Tildeop = 223,
    Timeout = 224,
    Timeoutlen = 225,
    Title = 226,
    Titlelen = 227,
    Titleold = 228,
    Titlestring = 229,
    Ttimeout = 230,
    Ttimeoutlen = 231,
    Ttyfast = 232,
    Undodir = 233,
    Undolevels = 234,
    Undoreload = 235,
    Updatecount = 236,
    Updatetime = 237,
    Verbose = 238,
    Verbosefile = 239,
    Viewdir = 240,
    Viewoptions = 241,
    Virtualedit = 242,
    Visualbell = 243,
    Warn = 244,
    Whichwrap = 245,
    Wildchar = 246,
    Wildcharm = 247,
    Wildignore = 248,
    Wildignorecase = 249,
    Wildmenu = 250,
    Wildmode = 251,
    Wildoptions = 252,
    Winaltkeys = 253,
    Winbar = 254,
    Winborder = 255,
    Window = 256,
    Winheight = 257,
    Winminheight = 258,
    Winminwidth = 259,
    Winwidth = 260,
    Wrapscan = 261,
    Write = 262,
    Writeany = 263,
    Writebackup = 264,
    Writedelay = 265,
}

pub const GLOBAL_OPT_COUNT: usize = 266;

/// Subset of [`OptIndex`] for buffer-local options (`BufOptIndex`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum BufOptIndex {
    Invalid = -1,
    Autocomplete = 0,
    Autoindent = 1,
    Autoread = 2,
    Backupcopy = 3,
    Binary = 4,
    Bomb = 5,
    Bufhidden = 6,
    Buflisted = 7,
    Buftype = 8,
    Busy = 9,
    Channel = 10,
    Cindent = 11,
    Cinkeys = 12,
    Cinoptions = 13,
    Cinscopedecls = 14,
    Cinwords = 15,
    Comments = 16,
    Commentstring = 17,
    Complete = 18,
    Completefunc = 19,
    Completeopt = 20,
    Completeslash = 21,
    Copyindent = 22,
    Define = 23,
    Dictionary = 24,
    Diffanchors = 25,
    Endoffile = 26,
    Endofline = 27,
    Equalprg = 28,
    Errorformat = 29,
    Expandtab = 30,
    Fileencoding = 31,
    Fileformat = 32,
    Filetype = 33,
    Findfunc = 34,
    Fixendofline = 35,
    Formatexpr = 36,
    Formatlistpat = 37,
    Formatoptions = 38,
    Formatprg = 39,
    Fsync = 40,
    Grepformat = 41,
    Grepprg = 42,
    Iminsert = 43,
    Imsearch = 44,
    Include = 45,
    Includeexpr = 46,
    Indentexpr = 47,
    Indentkeys = 48,
    Infercase = 49,
    Iskeyword = 50,
    Keymap = 51,
    Keywordprg = 52,
    Lisp = 53,
    Lispoptions = 54,
    Lispwords = 55,
    Makeencoding = 56,
    Makeprg = 57,
    Matchpairs = 58,
    Modeline = 59,
    Modifiable = 60,
    Modified = 61,
    Nrformats = 62,
    Omnifunc = 63,
    Path = 64,
    Preserveindent = 65,
    Quoteescape = 66,
    Readonly = 67,
    Scrollback = 68,
    Shiftwidth = 69,
    Smartindent = 70,
    Softtabstop = 71,
    Spellcapcheck = 72,
    Spellfile = 73,
    Spelllang = 74,
    Spelloptions = 75,
    Suffixesadd = 76,
    Swapfile = 77,
    Synmaxcol = 78,
    Syntax = 79,
    Tabstop = 80,
    Tagcase = 81,
    Tagfunc = 82,
    Tags = 83,
    Textwidth = 84,
    Thesaurus = 85,
    Thesaurusfunc = 86,
    Undofile = 87,
    Undolevels = 88,
    Varsofttabstop = 89,
    Vartabstop = 90,
    Wrapmargin = 91,
}

pub const BUF_OPT_COUNT: usize = 92;

/// Subset of [`OptIndex`] for window-local options (`WinOptIndex`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum WinOptIndex {
    Invalid = -1,
    Arabic = 0,
    Breakindent = 1,
    Breakindentopt = 2,
    Colorcolumn = 3,
    Concealcursor = 4,
    Conceallevel = 5,
    Cursorbind = 6,
    Cursorcolumn = 7,
    Cursorline = 8,
    Cursorlineopt = 9,
    Diff = 10,
    Eventignorewin = 11,
    Fillchars = 12,
    Foldcolumn = 13,
    Foldenable = 14,
    Foldexpr = 15,
    Foldignore = 16,
    Foldlevel = 17,
    Foldmarker = 18,
    Foldmethod = 19,
    Foldminlines = 20,
    Foldnestmax = 21,
    Foldtext = 22,
    Lhistory = 23,
    Linebreak = 24,
    List = 25,
    Listchars = 26,
    Number = 27,
    Numberwidth = 28,
    Previewwindow = 29,
    Relativenumber = 30,
    Rightleft = 31,
    Rightleftcmd = 32,
    Scroll = 33,
    Scrollbind = 34,
    Scrolloff = 35,
    Scrolloffpad = 36,
    Showbreak = 37,
    Sidescrolloff = 38,
    Signcolumn = 39,
    Smoothscroll = 40,
    Spell = 41,
    Statuscolumn = 42,
    Statusline = 43,
    Virtualedit = 44,
    Winbar = 45,
    Winblend = 46,
    Winfixbuf = 47,
    Winfixheight = 48,
    Winfixwidth = 49,
    Winhighlight = 50,
    Winpinned = 51,
    Wrap = 52,
}

pub const WIN_OPT_COUNT: usize = 53;

/// Subset of [`OptIndex`] for tabpage-local options (`TabOptIndex`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum TabOptIndex {
    Invalid = -1,
    Cmdheight = 0,
}

pub const TAB_OPT_COUNT: usize = 1;

/// Maps [`GlobalOptIndex`] to the corresponding [`OptIndex`]
/// (`global_opt_idx`). Built from the original's C designated-
/// initializer array (`[kGlobalOptFoo] = kOptFoo`) - reindexed by the
/// local enum's own real numeric value rather than assumed to already
/// be in ascending order in the source text (designated initializers
/// place each value at its own explicit index, regardless of textual
/// order).
pub const GLOBAL_OPT_IDX: &[OptIndex] = &[
    /* 0 */ OptIndex::Aleph,
    /* 1 */ OptIndex::Allowrevins,
    /* 2 */ OptIndex::Ambiwidth,
    /* 3 */ OptIndex::Arabicshape,
    /* 4 */ OptIndex::Autochdir,
    /* 5 */ OptIndex::Autocomplete,
    /* 6 */ OptIndex::Autocompletedelay,
    /* 7 */ OptIndex::Autocompletetimeout,
    /* 8 */ OptIndex::Autoread,
    /* 9 */ OptIndex::Autowrite,
    /* 10 */ OptIndex::Autowriteall,
    /* 11 */ OptIndex::Background,
    /* 12 */ OptIndex::Backspace,
    /* 13 */ OptIndex::Backup,
    /* 14 */ OptIndex::Backupcopy,
    /* 15 */ OptIndex::Backupdir,
    /* 16 */ OptIndex::Backupext,
    /* 17 */ OptIndex::Backupskip,
    /* 18 */ OptIndex::Belloff,
    /* 19 */ OptIndex::Breakat,
    /* 20 */ OptIndex::Browsedir,
    /* 21 */ OptIndex::Casemap,
    /* 22 */ OptIndex::Cdhome,
    /* 23 */ OptIndex::Cdpath,
    /* 24 */ OptIndex::Cedit,
    /* 25 */ OptIndex::Charconvert,
    /* 26 */ OptIndex::Chistory,
    /* 27 */ OptIndex::Clipboard,
    /* 28 */ OptIndex::Cmdheight,
    /* 29 */ OptIndex::Cmdwinheight,
    /* 30 */ OptIndex::Columns,
    /* 31 */ OptIndex::Compatible,
    /* 32 */ OptIndex::Completeitemalign,
    /* 33 */ OptIndex::Completeopt,
    /* 34 */ OptIndex::Completetimeout,
    /* 35 */ OptIndex::Confirm,
    /* 36 */ OptIndex::Cpoptions,
    /* 37 */ OptIndex::Debug,
    /* 38 */ OptIndex::Define,
    /* 39 */ OptIndex::Delcombine,
    /* 40 */ OptIndex::Dictionary,
    /* 41 */ OptIndex::Diffanchors,
    /* 42 */ OptIndex::Diffexpr,
    /* 43 */ OptIndex::Diffopt,
    /* 44 */ OptIndex::Digraph,
    /* 45 */ OptIndex::Directory,
    /* 46 */ OptIndex::Display,
    /* 47 */ OptIndex::Eadirection,
    /* 48 */ OptIndex::Edcompatible,
    /* 49 */ OptIndex::Emoji,
    /* 50 */ OptIndex::Encoding,
    /* 51 */ OptIndex::Equalalways,
    /* 52 */ OptIndex::Equalprg,
    /* 53 */ OptIndex::Errorbells,
    /* 54 */ OptIndex::Errorfile,
    /* 55 */ OptIndex::Errorformat,
    /* 56 */ OptIndex::Eventignore,
    /* 57 */ OptIndex::Exrc,
    /* 58 */ OptIndex::Fileencodings,
    /* 59 */ OptIndex::Fileformats,
    /* 60 */ OptIndex::Fileignorecase,
    /* 61 */ OptIndex::Fillchars,
    /* 62 */ OptIndex::Findfunc,
    /* 63 */ OptIndex::Foldclose,
    /* 64 */ OptIndex::Foldlevelstart,
    /* 65 */ OptIndex::Foldopen,
    /* 66 */ OptIndex::Formatprg,
    /* 67 */ OptIndex::Fsync,
    /* 68 */ OptIndex::Gdefault,
    /* 69 */ OptIndex::Grepformat,
    /* 70 */ OptIndex::Grepprg,
    /* 71 */ OptIndex::Guicursor,
    /* 72 */ OptIndex::Guifont,
    /* 73 */ OptIndex::Guifontwide,
    /* 74 */ OptIndex::Guioptions,
    /* 75 */ OptIndex::Guitablabel,
    /* 76 */ OptIndex::Guitabtooltip,
    /* 77 */ OptIndex::Helpfile,
    /* 78 */ OptIndex::Helpheight,
    /* 79 */ OptIndex::Helplang,
    /* 80 */ OptIndex::Hidden,
    /* 81 */ OptIndex::Highlight,
    /* 82 */ OptIndex::History,
    /* 83 */ OptIndex::Hkmap,
    /* 84 */ OptIndex::Hkmapp,
    /* 85 */ OptIndex::Hlsearch,
    /* 86 */ OptIndex::Icon,
    /* 87 */ OptIndex::Iconstring,
    /* 88 */ OptIndex::Ignorecase,
    /* 89 */ OptIndex::Imcmdline,
    /* 90 */ OptIndex::Imdisable,
    /* 91 */ OptIndex::Inccommand,
    /* 92 */ OptIndex::Include,
    /* 93 */ OptIndex::Incsearch,
    /* 94 */ OptIndex::Insertmode,
    /* 95 */ OptIndex::Isfname,
    /* 96 */ OptIndex::Isident,
    /* 97 */ OptIndex::Isprint,
    /* 98 */ OptIndex::Joinspaces,
    /* 99 */ OptIndex::Jumpoptions,
    /* 100 */ OptIndex::Keymodel,
    /* 101 */ OptIndex::Keywordprg,
    /* 102 */ OptIndex::Langmap,
    /* 103 */ OptIndex::Langmenu,
    /* 104 */ OptIndex::Langnoremap,
    /* 105 */ OptIndex::Langremap,
    /* 106 */ OptIndex::Laststatus,
    /* 107 */ OptIndex::Lazyredraw,
    /* 108 */ OptIndex::Lines,
    /* 109 */ OptIndex::Linespace,
    /* 110 */ OptIndex::Lispwords,
    /* 111 */ OptIndex::Listchars,
    /* 112 */ OptIndex::Loadplugins,
    /* 113 */ OptIndex::Magic,
    /* 114 */ OptIndex::Makeef,
    /* 115 */ OptIndex::Makeencoding,
    /* 116 */ OptIndex::Makeprg,
    /* 117 */ OptIndex::Matchtime,
    /* 118 */ OptIndex::Maxcombine,
    /* 119 */ OptIndex::Maxfuncdepth,
    /* 120 */ OptIndex::Maxmapdepth,
    /* 121 */ OptIndex::Maxmempattern,
    /* 122 */ OptIndex::Maxsearchcount,
    /* 123 */ OptIndex::Menuitems,
    /* 124 */ OptIndex::Messagesopt,
    /* 125 */ OptIndex::Mkspellmem,
    /* 126 */ OptIndex::Modelineexpr,
    /* 127 */ OptIndex::Modelines,
    /* 128 */ OptIndex::More,
    /* 129 */ OptIndex::Mouse,
    /* 130 */ OptIndex::Mousefocus,
    /* 131 */ OptIndex::Mousehide,
    /* 132 */ OptIndex::Mousemodel,
    /* 133 */ OptIndex::Mousemoveevent,
    /* 134 */ OptIndex::Mousescroll,
    /* 135 */ OptIndex::Mouseshape,
    /* 136 */ OptIndex::Mousetime,
    /* 137 */ OptIndex::Opendevice,
    /* 138 */ OptIndex::Operatorfunc,
    /* 139 */ OptIndex::Packlockfile,
    /* 140 */ OptIndex::Packpath,
    /* 141 */ OptIndex::Paragraphs,
    /* 142 */ OptIndex::Paste,
    /* 143 */ OptIndex::Pastetoggle,
    /* 144 */ OptIndex::Patchexpr,
    /* 145 */ OptIndex::Patchmode,
    /* 146 */ OptIndex::Path,
    /* 147 */ OptIndex::Previewheight,
    /* 148 */ OptIndex::Prompt,
    /* 149 */ OptIndex::Pumblend,
    /* 150 */ OptIndex::Pumborder,
    /* 151 */ OptIndex::Pumheight,
    /* 152 */ OptIndex::Pummaxwidth,
    /* 153 */ OptIndex::Pumwidth,
    /* 154 */ OptIndex::Pyxversion,
    /* 155 */ OptIndex::Quickfixtextfunc,
    /* 156 */ OptIndex::Redrawdebug,
    /* 157 */ OptIndex::Redrawtime,
    /* 158 */ OptIndex::Regexpengine,
    /* 159 */ OptIndex::Remap,
    /* 160 */ OptIndex::Report,
    /* 161 */ OptIndex::Revins,
    /* 162 */ OptIndex::Ruler,
    /* 163 */ OptIndex::Rulerformat,
    /* 164 */ OptIndex::Runtimepath,
    /* 165 */ OptIndex::Scrolljump,
    /* 166 */ OptIndex::Scrolloff,
    /* 167 */ OptIndex::Scrolloffpad,
    /* 168 */ OptIndex::Scrollopt,
    /* 169 */ OptIndex::Sections,
    /* 170 */ OptIndex::Secure,
    /* 171 */ OptIndex::Selection,
    /* 172 */ OptIndex::Selectmode,
    /* 173 */ OptIndex::Sessionoptions,
    /* 174 */ OptIndex::Shada,
    /* 175 */ OptIndex::Shadafile,
    /* 176 */ OptIndex::Shell,
    /* 177 */ OptIndex::Shellcmdflag,
    /* 178 */ OptIndex::Shellpipe,
    /* 179 */ OptIndex::Shellquote,
    /* 180 */ OptIndex::Shellredir,
    /* 181 */ OptIndex::Shellslash,
    /* 182 */ OptIndex::Shelltemp,
    /* 183 */ OptIndex::Shellxescape,
    /* 184 */ OptIndex::Shellxquote,
    /* 185 */ OptIndex::Shiftround,
    /* 186 */ OptIndex::Shortmess,
    /* 187 */ OptIndex::Showbreak,
    /* 188 */ OptIndex::Showcmd,
    /* 189 */ OptIndex::Showcmdloc,
    /* 190 */ OptIndex::Showfulltag,
    /* 191 */ OptIndex::Showmatch,
    /* 192 */ OptIndex::Showmode,
    /* 193 */ OptIndex::Showtabline,
    /* 194 */ OptIndex::Sidescroll,
    /* 195 */ OptIndex::Sidescrolloff,
    /* 196 */ OptIndex::Smartcase,
    /* 197 */ OptIndex::Smarttab,
    /* 198 */ OptIndex::Spellsuggest,
    /* 199 */ OptIndex::Splitbelow,
    /* 200 */ OptIndex::Splitkeep,
    /* 201 */ OptIndex::Splitright,
    /* 202 */ OptIndex::Startofline,
    /* 203 */ OptIndex::Statusline,
    /* 204 */ OptIndex::Suffixes,
    /* 205 */ OptIndex::Switchbuf,
    /* 206 */ OptIndex::Tabclose,
    /* 207 */ OptIndex::Tabline,
    /* 208 */ OptIndex::Tabpagemax,
    /* 209 */ OptIndex::Tagbsearch,
    /* 210 */ OptIndex::Tagcase,
    /* 211 */ OptIndex::Taglength,
    /* 212 */ OptIndex::Tagrelative,
    /* 213 */ OptIndex::Tags,
    /* 214 */ OptIndex::Tagstack,
    /* 215 */ OptIndex::Termbidi,
    /* 216 */ OptIndex::Termencoding,
    /* 217 */ OptIndex::Termguicolors,
    /* 218 */ OptIndex::Termpastefilter,
    /* 219 */ OptIndex::Termsync,
    /* 220 */ OptIndex::Terse,
    /* 221 */ OptIndex::Thesaurus,
    /* 222 */ OptIndex::Thesaurusfunc,
    /* 223 */ OptIndex::Tildeop,
    /* 224 */ OptIndex::Timeout,
    /* 225 */ OptIndex::Timeoutlen,
    /* 226 */ OptIndex::Title,
    /* 227 */ OptIndex::Titlelen,
    /* 228 */ OptIndex::Titleold,
    /* 229 */ OptIndex::Titlestring,
    /* 230 */ OptIndex::Ttimeout,
    /* 231 */ OptIndex::Ttimeoutlen,
    /* 232 */ OptIndex::Ttyfast,
    /* 233 */ OptIndex::Undodir,
    /* 234 */ OptIndex::Undolevels,
    /* 235 */ OptIndex::Undoreload,
    /* 236 */ OptIndex::Updatecount,
    /* 237 */ OptIndex::Updatetime,
    /* 238 */ OptIndex::Verbose,
    /* 239 */ OptIndex::Verbosefile,
    /* 240 */ OptIndex::Viewdir,
    /* 241 */ OptIndex::Viewoptions,
    /* 242 */ OptIndex::Virtualedit,
    /* 243 */ OptIndex::Visualbell,
    /* 244 */ OptIndex::Warn,
    /* 245 */ OptIndex::Whichwrap,
    /* 246 */ OptIndex::Wildchar,
    /* 247 */ OptIndex::Wildcharm,
    /* 248 */ OptIndex::Wildignore,
    /* 249 */ OptIndex::Wildignorecase,
    /* 250 */ OptIndex::Wildmenu,
    /* 251 */ OptIndex::Wildmode,
    /* 252 */ OptIndex::Wildoptions,
    /* 253 */ OptIndex::Winaltkeys,
    /* 254 */ OptIndex::Winbar,
    /* 255 */ OptIndex::Winborder,
    /* 256 */ OptIndex::Window,
    /* 257 */ OptIndex::Winheight,
    /* 258 */ OptIndex::Winminheight,
    /* 259 */ OptIndex::Winminwidth,
    /* 260 */ OptIndex::Winwidth,
    /* 261 */ OptIndex::Wrapscan,
    /* 262 */ OptIndex::Write,
    /* 263 */ OptIndex::Writeany,
    /* 264 */ OptIndex::Writebackup,
    /* 265 */ OptIndex::Writedelay,
];

/// Maps [`BufOptIndex`] to the corresponding [`OptIndex`]
/// (`buf_opt_idx`). See [`GLOBAL_OPT_IDX`]'s own doc comment for how
/// this was built.
pub const BUF_OPT_IDX: &[OptIndex] = &[
    /* 0 */ OptIndex::Autocomplete,
    /* 1 */ OptIndex::Autoindent,
    /* 2 */ OptIndex::Autoread,
    /* 3 */ OptIndex::Backupcopy,
    /* 4 */ OptIndex::Binary,
    /* 5 */ OptIndex::Bomb,
    /* 6 */ OptIndex::Bufhidden,
    /* 7 */ OptIndex::Buflisted,
    /* 8 */ OptIndex::Buftype,
    /* 9 */ OptIndex::Busy,
    /* 10 */ OptIndex::Channel,
    /* 11 */ OptIndex::Cindent,
    /* 12 */ OptIndex::Cinkeys,
    /* 13 */ OptIndex::Cinoptions,
    /* 14 */ OptIndex::Cinscopedecls,
    /* 15 */ OptIndex::Cinwords,
    /* 16 */ OptIndex::Comments,
    /* 17 */ OptIndex::Commentstring,
    /* 18 */ OptIndex::Complete,
    /* 19 */ OptIndex::Completefunc,
    /* 20 */ OptIndex::Completeopt,
    /* 21 */ OptIndex::Completeslash,
    /* 22 */ OptIndex::Copyindent,
    /* 23 */ OptIndex::Define,
    /* 24 */ OptIndex::Dictionary,
    /* 25 */ OptIndex::Diffanchors,
    /* 26 */ OptIndex::Endoffile,
    /* 27 */ OptIndex::Endofline,
    /* 28 */ OptIndex::Equalprg,
    /* 29 */ OptIndex::Errorformat,
    /* 30 */ OptIndex::Expandtab,
    /* 31 */ OptIndex::Fileencoding,
    /* 32 */ OptIndex::Fileformat,
    /* 33 */ OptIndex::Filetype,
    /* 34 */ OptIndex::Findfunc,
    /* 35 */ OptIndex::Fixendofline,
    /* 36 */ OptIndex::Formatexpr,
    /* 37 */ OptIndex::Formatlistpat,
    /* 38 */ OptIndex::Formatoptions,
    /* 39 */ OptIndex::Formatprg,
    /* 40 */ OptIndex::Fsync,
    /* 41 */ OptIndex::Grepformat,
    /* 42 */ OptIndex::Grepprg,
    /* 43 */ OptIndex::Iminsert,
    /* 44 */ OptIndex::Imsearch,
    /* 45 */ OptIndex::Include,
    /* 46 */ OptIndex::Includeexpr,
    /* 47 */ OptIndex::Indentexpr,
    /* 48 */ OptIndex::Indentkeys,
    /* 49 */ OptIndex::Infercase,
    /* 50 */ OptIndex::Iskeyword,
    /* 51 */ OptIndex::Keymap,
    /* 52 */ OptIndex::Keywordprg,
    /* 53 */ OptIndex::Lisp,
    /* 54 */ OptIndex::Lispoptions,
    /* 55 */ OptIndex::Lispwords,
    /* 56 */ OptIndex::Makeencoding,
    /* 57 */ OptIndex::Makeprg,
    /* 58 */ OptIndex::Matchpairs,
    /* 59 */ OptIndex::Modeline,
    /* 60 */ OptIndex::Modifiable,
    /* 61 */ OptIndex::Modified,
    /* 62 */ OptIndex::Nrformats,
    /* 63 */ OptIndex::Omnifunc,
    /* 64 */ OptIndex::Path,
    /* 65 */ OptIndex::Preserveindent,
    /* 66 */ OptIndex::Quoteescape,
    /* 67 */ OptIndex::Readonly,
    /* 68 */ OptIndex::Scrollback,
    /* 69 */ OptIndex::Shiftwidth,
    /* 70 */ OptIndex::Smartindent,
    /* 71 */ OptIndex::Softtabstop,
    /* 72 */ OptIndex::Spellcapcheck,
    /* 73 */ OptIndex::Spellfile,
    /* 74 */ OptIndex::Spelllang,
    /* 75 */ OptIndex::Spelloptions,
    /* 76 */ OptIndex::Suffixesadd,
    /* 77 */ OptIndex::Swapfile,
    /* 78 */ OptIndex::Synmaxcol,
    /* 79 */ OptIndex::Syntax,
    /* 80 */ OptIndex::Tabstop,
    /* 81 */ OptIndex::Tagcase,
    /* 82 */ OptIndex::Tagfunc,
    /* 83 */ OptIndex::Tags,
    /* 84 */ OptIndex::Textwidth,
    /* 85 */ OptIndex::Thesaurus,
    /* 86 */ OptIndex::Thesaurusfunc,
    /* 87 */ OptIndex::Undofile,
    /* 88 */ OptIndex::Undolevels,
    /* 89 */ OptIndex::Varsofttabstop,
    /* 90 */ OptIndex::Vartabstop,
    /* 91 */ OptIndex::Wrapmargin,
];

/// Maps [`WinOptIndex`] to the corresponding [`OptIndex`]
/// (`win_opt_idx`). See [`GLOBAL_OPT_IDX`]'s own doc comment for how
/// this was built.
pub const WIN_OPT_IDX: &[OptIndex] = &[
    /* 0 */ OptIndex::Arabic,
    /* 1 */ OptIndex::Breakindent,
    /* 2 */ OptIndex::Breakindentopt,
    /* 3 */ OptIndex::Colorcolumn,
    /* 4 */ OptIndex::Concealcursor,
    /* 5 */ OptIndex::Conceallevel,
    /* 6 */ OptIndex::Cursorbind,
    /* 7 */ OptIndex::Cursorcolumn,
    /* 8 */ OptIndex::Cursorline,
    /* 9 */ OptIndex::Cursorlineopt,
    /* 10 */ OptIndex::Diff,
    /* 11 */ OptIndex::Eventignorewin,
    /* 12 */ OptIndex::Fillchars,
    /* 13 */ OptIndex::Foldcolumn,
    /* 14 */ OptIndex::Foldenable,
    /* 15 */ OptIndex::Foldexpr,
    /* 16 */ OptIndex::Foldignore,
    /* 17 */ OptIndex::Foldlevel,
    /* 18 */ OptIndex::Foldmarker,
    /* 19 */ OptIndex::Foldmethod,
    /* 20 */ OptIndex::Foldminlines,
    /* 21 */ OptIndex::Foldnestmax,
    /* 22 */ OptIndex::Foldtext,
    /* 23 */ OptIndex::Lhistory,
    /* 24 */ OptIndex::Linebreak,
    /* 25 */ OptIndex::List,
    /* 26 */ OptIndex::Listchars,
    /* 27 */ OptIndex::Number,
    /* 28 */ OptIndex::Numberwidth,
    /* 29 */ OptIndex::Previewwindow,
    /* 30 */ OptIndex::Relativenumber,
    /* 31 */ OptIndex::Rightleft,
    /* 32 */ OptIndex::Rightleftcmd,
    /* 33 */ OptIndex::Scroll,
    /* 34 */ OptIndex::Scrollbind,
    /* 35 */ OptIndex::Scrolloff,
    /* 36 */ OptIndex::Scrolloffpad,
    /* 37 */ OptIndex::Showbreak,
    /* 38 */ OptIndex::Sidescrolloff,
    /* 39 */ OptIndex::Signcolumn,
    /* 40 */ OptIndex::Smoothscroll,
    /* 41 */ OptIndex::Spell,
    /* 42 */ OptIndex::Statuscolumn,
    /* 43 */ OptIndex::Statusline,
    /* 44 */ OptIndex::Virtualedit,
    /* 45 */ OptIndex::Winbar,
    /* 46 */ OptIndex::Winblend,
    /* 47 */ OptIndex::Winfixbuf,
    /* 48 */ OptIndex::Winfixheight,
    /* 49 */ OptIndex::Winfixwidth,
    /* 50 */ OptIndex::Winhighlight,
    /* 51 */ OptIndex::Winpinned,
    /* 52 */ OptIndex::Wrap,
];

/// Maps [`TabOptIndex`] to the corresponding [`OptIndex`]
/// (`tab_opt_idx`). See [`GLOBAL_OPT_IDX`]'s own doc comment for how
/// this was built.
pub const TAB_OPT_IDX: &[OptIndex] = &[
    /* 0 */ OptIndex::Cmdheight,
];

#[cfg(test)]
mod options_enum_tests {
    use super::*;

    #[test]
    fn opt_count_matches_the_generated_header() {
        assert_eq!(OPT_COUNT, 377);
        assert_eq!(OptIndex::Invalid as i32, -1);
        assert_eq!(OptIndex::Aleph as i32, 0); // first real entry
        assert_eq!(OptIndex::Writedelay as i32, 376); // last real entry
    }

    #[test]
    fn global_opt_count_matches_table_and_enum() {
        assert_eq!(GLOBAL_OPT_COUNT, 266);
        assert_eq!(GLOBAL_OPT_IDX.len(), GLOBAL_OPT_COUNT);
        assert_eq!(GlobalOptIndex::Invalid as i32, -1);
        assert_eq!(GlobalOptIndex::Writedelay as i32, 265); // last real entry
    }

    #[test]
    fn buf_opt_count_matches_table_and_enum() {
        assert_eq!(BUF_OPT_COUNT, 92);
        assert_eq!(BUF_OPT_IDX.len(), BUF_OPT_COUNT);
        assert_eq!(BufOptIndex::Invalid as i32, -1);
        // BufOptIndex::Autocomplete = 0 maps to OptIndex::Autocomplete
        // ([kBufOptAutocomplete] = kOptAutocomplete in the original).
        assert_eq!(BUF_OPT_IDX[BufOptIndex::Autocomplete as usize], OptIndex::Autocomplete);
        assert_eq!(BUF_OPT_IDX[BufOptIndex::Wrapmargin as usize], OptIndex::Wrapmargin);
    }

    #[test]
    fn win_opt_count_matches_table_and_enum() {
        assert_eq!(WIN_OPT_COUNT, 53);
        assert_eq!(WIN_OPT_IDX.len(), WIN_OPT_COUNT);
        assert_eq!(WinOptIndex::Invalid as i32, -1);
        assert_eq!(WIN_OPT_IDX[WinOptIndex::Arabic as usize], OptIndex::Arabic);
        assert_eq!(WIN_OPT_IDX[WinOptIndex::Wrap as usize], OptIndex::Wrap);
    }

    #[test]
    fn tab_opt_maps_cmdheight() {
        assert_eq!(TAB_OPT_COUNT, 1);
        assert_eq!(TAB_OPT_IDX, &[OptIndex::Cmdheight]);
        assert_eq!(TabOptIndex::Invalid as i32, -1);
        assert_eq!(TabOptIndex::Cmdheight as i32, 0);
    }
}
