//! Translated from `src/nvim/vim_defs.h`.

// Some defines from the old feature.h
pub const SESSION_FILE: &str = "Session.vim";
pub const SYS_OPTWIN_FILE: &str = "$VIMRUNTIME/scripts/optwin.lua";
pub const RUNTIME_DIRNAME: &str = "runtime";

/// length of a buffer to store a number in ASCII (64 bits binary + NUL)
pub const NUMBUFLEN: usize = 65;

pub const MAX_TYPENR: i32 = 65535;

/// Directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum Direction {
    NotSet = 0,
    Forward = 1,
    Backward = -1,
    ForwardFile = 3,
    BackwardFile = -3,
}

/// Used to track the status of external functions.
/// Currently only used for `iconv()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkingStatus {
    Unknown,
    Working,
    Broken,
}

/// The scope of a working-directory command like `:cd`.
///
/// Scopes are enumerated from lowest to highest. When adding a scope make
/// sure to update all functions using scopes as well, such as the
/// implementation of `getcwd()`. When using scopes as limits (e.g. in loops)
/// don't use the scopes directly, use [`CdScope::MIN`] and [`CdScope::MAX`]
/// instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i8)]
pub enum CdScope {
    Invalid = -1,
    /// Affects one window.
    Window = 0,
    /// Affects one tab page.
    Tabpage = 1,
    /// Affects the entire Nvim instance.
    Global = 2,
}

impl CdScope {
    /// `MIN_CD_SCOPE`
    pub const MIN: CdScope = CdScope::Window;
    /// `MAX_CD_SCOPE`
    pub const MAX: CdScope = CdScope::Global;
}

/// What caused the current directory to change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum CdCause {
    Other = -1,
    /// Using `:cd`, `:tcd`, `:lcd` or `chdir()`.
    Manual = 0,
    /// Switching to another window.
    Window = 1,
    /// On `'autochdir'`.
    Auto = 2,
}

// return values for functions
/// `OK`
pub const OK: i32 = 1;
/// `FAIL`
pub const FAIL: i32 = 0;
/// not OK or FAIL but skipped (`NOTDONE`)
pub const NOTDONE: i32 = 2;
