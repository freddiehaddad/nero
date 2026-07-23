//! Translated from `src/nvim/context_defs.h` (partial: struct/enum
//! shapes only - see `src/context.rs`'s own module doc for the actual
//! functions).
//!
//! `Context`/`ContextVec` (the `:mkview`/`context`-API snapshot format:
//! registers/jumplist/buffer-list/global-variables/functions, each as
//! a serialized `String`/`Array`) are translated as struct shapes
//! only, since nothing in this crate can currently populate one for
//! real (needs the eval engine's variable-serialization machinery and
//! more of the API layer than currently exists), matching this
//! crate's established "structure now, populate/engine later" pattern.
//!
//! `CtxSwitch` (the temporary-window/buffer-switch state saved/restored
//! by `ctx_switch()`/`ctx_restore()`) is translated in full: every
//! field is an already-tractable type (`handle_T`/`HandleT`,
//! `tabpage_T*`/`*mut TabpageT`, `bufref_T`/`BufrefT`, `pos_T`/`PosT`,
//! `char*`/`Option<Vec<u8>>`, `int`/`bool`), so there was no reason to
//! defer any individual field even though `ctx_switch()`/`ctx_restore()`
//! themselves are only partially translated (see `src/context.rs`).
//!
//! `CtxSwitchFlags` (`kCtx*`) are OR-able bit flags (multiple can apply
//! to the same switch simultaneously, e.g. `kCtxNoDisplay | kCtxValidate`),
//! kept as plain `i32` constants rather than a Rust `enum`, matching
//! this crate's established convention for OR-able C bit-flag sets
//! (e.g. `eval/typval_defs.rs`'s `dict_item_flags`, `option_defs.rs`'s
//! `opt_flags`). `cs_mode`'s `kCtxSwitch*` values, by contrast, are
//! mutually exclusive (a single switch is EITHER none, a window
//! target, or a buffer target), translated as a proper Rust enum,
//! [`CtxSwitchMode`], matching e.g. `eval/typval_defs.rs`'s `ScopeType`
//! (the same real-world "which one, not which combination" shape).

use crate::buffer_defs::{BufrefT, TabpageT};
use crate::pos_defs::PosT;
use crate::types_defs::HandleT;

/// One `:mkview`/context-API snapshot (`Context`).
pub struct Context {
    /// Registers (`regs`).
    pub regs: Option<Vec<u8>>,
    /// Jumplist (`jumps`).
    pub jumps: Option<Vec<u8>>,
    /// Buffer list (`bufs`).
    pub bufs: Option<Vec<u8>>,
    /// Global variables (`gvars`).
    pub gvars: Option<Vec<u8>>,
    /// Functions (`funcs`).
    pub funcs: Vec<crate::api::private::defs::Object>,
}

/// A vector of [`Context`]s (`ContextVec`).
pub type ContextVec = Vec<Context>;

/// Values for [`Context`]'s (still-deferred, see this module's own doc
/// comment) serialization scope (`CtxStateFlags`) - OR-able bit flags,
/// same reasoning as [`ctx_switch_flags`].
pub mod ctx_state_flags {
    /// Registers (`kCtxRegs`).
    pub const REGS: i32 = 1;
    /// Jumplist (`kCtxJumps`).
    pub const JUMPS: i32 = 2;
    /// Buffer list (`kCtxBufs`).
    pub const BUFS: i32 = 4;
    /// Global variables (`kCtxGVars`).
    pub const GVARS: i32 = 8;
    /// Script functions (`kCtxSFuncs`).
    pub const SFUNCS: i32 = 16;
    /// Functions (`kCtxFuncs`).
    pub const FUNCS: i32 = 32;
}

/// Temporary, hidden window (fka "autocmd window"): a pooled window
/// created to temporarily show a buffer that has no window
/// (`ctx_switch()` on a buffer target), to handle the side effects.
/// When switches nest, more than one may be needed (`CtxWin`).
#[derive(Debug, Clone, Copy, Default)]
pub struct CtxWin {
    /// The window, or null if not yet allocated (`cw_win`).
    pub cw_win: *mut crate::buffer_defs::WinT,
    /// Not currently in use (`cw_used`).
    pub cw_used: bool,
}

/// Flags for `ctx_switch()` (`CtxSwitchFlags`) - OR-able bit flags,
/// kept as plain `i32` constants; see this module's own doc comment
/// for why.
pub mod ctx_switch_flags {
    /// Don't affect the display (no redraw; limits access to another
    /// tabpage) (`kCtxNoDisplay`).
    pub const NO_DISPLAY: i32 = 1;
    /// Block autocommands until `ctx_restore()` (`kCtxNoEvents`).
    pub const NO_EVENTS: i32 = 2;
    /// Undo any chdir caused by the switch (`'autochdir'`, win/tab-local
    /// CWD) on `ctx_restore()` (`kCtxKeepCwd`).
    pub const KEEP_CWD: i32 = 4;
    /// Validate cursor/Visual around the switch; update display
    /// (statusline) if the target window's cursor moved (`kCtxValidate`).
    pub const VALIDATE: i32 = 8;
}

/// What `ctx_switch()` switched (set internally) - mutually exclusive,
/// unlike [`ctx_switch_flags`]'s OR-able bits; see this module's own
/// doc comment for why this one is a proper enum (`CtxSwitch.cs_mode`'s
/// anonymous `enum { kCtxSwitchNone, kCtxSwitchWin, kCtxSwitchBuf }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CtxSwitchMode {
    /// zero-initialized: `ctx_restore()` is a no-op (`kCtxSwitchNone`).
    #[default]
    None = 0,
    /// window target (`kCtxSwitchWin`).
    Win = 1,
    /// buffer target (`kCtxSwitchBuf`).
    Buf = 2,
}

/// Context before a temporary switch of current window/buffer. Undone
/// by `ctx_restore()` (`CtxSwitch`).
#[derive(Debug, Clone, Default)]
pub struct CtxSwitch {
    /// `kCtx*` flags of the switch, see [`ctx_switch_flags`]
    /// (`cs_flags`).
    pub cs_flags: i32,
    /// what was switched, see [`CtxSwitchMode`] (`cs_mode`).
    pub cs_mode: CtxSwitchMode,
    // Saved location:
    /// saved curwin (`cs_curwin`).
    pub cs_curwin: HandleT,
    /// saved prevwin (`ctx_switch()`) (`cs_prevwin`).
    pub cs_prevwin: HandleT,
    /// saved curtab (null: tabpage unchanged) (`cs_curtab`).
    pub cs_curtab: *mut TabpageT,
    /// `Visual.active` was not reset (`cs_same_win`).
    pub cs_same_win: bool,
    /// saved `Visual.active` (`cs_visual_active`).
    pub cs_visual_active: bool,
    /// saved `b_prompt_insert` (`cs_prompt_insert`).
    pub cs_prompt_insert: i32,
    // Temporary location (ctx_switch()):
    /// ID of new curwin (`cs_new_curwin`).
    pub cs_new_curwin: HandleT,
    /// new curbuf (`cs_new_curbuf`).
    pub cs_new_curbuf: BufrefT,
    /// autocmd window in `ctx_win[]`, or -1 (`cs_ctxwin_idx`).
    pub cs_ctxwin_idx: i32,
    // Target tracking (kCtxValidate):
    /// the window switched to (`cs_target_win`).
    pub cs_target_win: HandleT,
    /// its cursor before the switch (`cs_target_old_pos`).
    pub cs_target_old_pos: PosT,
    // State kept across the switch:
    /// saved `tp_localdir` (autocmd window) (`cs_tp_localdir`).
    pub cs_tp_localdir: Option<Vec<u8>>,
    /// saved `globaldir` (autocmd window) (`cs_globaldir`).
    pub cs_globaldir: Option<Vec<u8>>,
    /// saved cwd (`kCtxKeepCwd`; allocated on demand) (`cs_cwd`).
    pub cs_cwd: Option<Vec<u8>>,
    /// `OK` if `cs_cwd` is valid (`cs_cwd_status`).
    pub cs_cwd_status: i32,
    /// re-apply `'autochdir'` on `ctx_restore()` (`cs_apply_acd`).
    pub cs_apply_acd: bool,
    /// saved `b_sfname` (`kCtxKeepCwd`) (`cs_save_sfname`).
    pub cs_save_sfname: Option<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctx_switch_mode_default_is_none() {
        assert_eq!(CtxSwitchMode::default(), CtxSwitchMode::None);
    }

    #[test]
    fn ctx_switch_mode_discriminants_match_c_enum() {
        assert_eq!(CtxSwitchMode::None as i32, 0);
        assert_eq!(CtxSwitchMode::Win as i32, 1);
        assert_eq!(CtxSwitchMode::Buf as i32, 2);
    }

    #[test]
    fn ctx_switch_flags_match_c_macros() {
        assert_eq!(ctx_switch_flags::NO_DISPLAY, 1);
        assert_eq!(ctx_switch_flags::NO_EVENTS, 2);
        assert_eq!(ctx_switch_flags::KEEP_CWD, 4);
        assert_eq!(ctx_switch_flags::VALIDATE, 8);
    }

    #[test]
    fn ctx_switch_default_is_zeroed_with_none_mode() {
        let cs = CtxSwitch::default();
        assert_eq!(cs.cs_mode, CtxSwitchMode::None);
        assert_eq!(cs.cs_flags, 0);
        assert!(cs.cs_curtab.is_null());
        assert!(!cs.cs_same_win);
        assert!(cs.cs_tp_localdir.is_none());
        assert_eq!(cs.cs_ctxwin_idx, 0);
    }

    #[test]
    fn ctx_win_default_is_zeroed_and_unused() {
        let cw = CtxWin::default();
        assert!(cw.cw_win.is_null());
        assert!(!cw.cw_used);
    }

    #[test]
    fn ctx_state_flags_match_c_macros() {
        assert_eq!(ctx_state_flags::REGS, 1);
        assert_eq!(ctx_state_flags::JUMPS, 2);
        assert_eq!(ctx_state_flags::BUFS, 4);
        assert_eq!(ctx_state_flags::GVARS, 8);
        assert_eq!(ctx_state_flags::SFUNCS, 16);
        assert_eq!(ctx_state_flags::FUNCS, 32);
    }
}
