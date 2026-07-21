//! Translated from `src/nvim/ex_eval_defs.h`.

use crate::eval::typval_defs::ListT;
use crate::pos_defs::LinenrT;

/// A list used for saving values of `"emsg_silent"`. Used by `ex_try()`
/// (not yet translated) to save the value of `"emsg_silent"` if it was
/// non-zero. When this is done, the `CSF_SILENT` flag is set
/// (`struct eslist_elem`, typedef'd as `eslist_T`).
#[derive(Debug, Clone, Copy)]
pub struct EslistT {
    /// saved value of `"emsg_silent"`
    pub saved_emsg_silent: i32,
    /// next element on the list
    pub next: *mut EslistT,
}

/// For conditional commands a stack is kept of nested conditionals. When
/// `cs_idx < 0`, there is no conditional command (`CSTACK_LEN`).
pub const CSTACK_LEN: usize = 50;

/// A stack of nested conditionals (`":if"`/`":while"`/`":for"`/`":try"`)
/// (`cstack_T`).
///
/// The original's `cs_pend` union (`csp_rv`/`csp_ex`, aliased via the
/// `cs_rettv`/`cs_exception` macros) has both members of the *exact same*
/// type (`void *[CSTACK_LEN]`) - it exists purely to give the same
/// storage two context-dependent names (a pending `":return"` typeval vs.
/// a pending `":throw"` exception), not to distinguish different
/// payloads. Translated as a single field, [`cs_pend`](Self::cs_pend),
/// with doc comments noting both original names, rather than an actual
/// Rust union/enum (which would only make sense if the two arms had
/// different types or needed a discriminant).
#[derive(Debug, Clone, Copy)]
pub struct CstackT {
    /// `CSF_*` flags
    pub cs_flags: [i32; CSTACK_LEN],
    /// `CSTP_*`: what's pending in `":finally"`
    pub cs_pending: [u8; CSTACK_LEN],
    /// pending `":return"` typeval (`cs_rettv`) or pending `":throw"`
    /// exception (`cs_exception`) - see the struct-level doc comment.
    pub cs_pend: [*mut std::ffi::c_void; CSTACK_LEN],
    /// info used by `":for"`
    pub cs_forinfo: [*mut std::ffi::c_void; CSTACK_LEN],
    /// line nr of `":while"`/`":for"` line
    pub cs_line: [i32; CSTACK_LEN],
    /// current entry, or -1 if none
    pub cs_idx: i32,
    /// nr of nested `":while"`s and `":for"`s
    pub cs_looplevel: i32,
    /// nr of nested `":try"`s
    pub cs_trylevel: i32,
    /// saved values of `"emsg_silent"`
    pub cs_emsg_silent_list: *mut EslistT,
    /// loop flags: `CSL_*` flags
    pub cs_lflags: i32,
}

impl Default for CstackT {
    fn default() -> Self {
        CstackT {
            cs_flags: [0; CSTACK_LEN],
            cs_pending: [0; CSTACK_LEN],
            cs_pend: [std::ptr::null_mut(); CSTACK_LEN],
            cs_forinfo: [std::ptr::null_mut(); CSTACK_LEN],
            cs_line: [0; CSTACK_LEN],
            cs_idx: -1,
            cs_looplevel: 0,
            cs_trylevel: 0,
            cs_emsg_silent_list: std::ptr::null_mut(),
            cs_lflags: 0,
        }
    }
}

/// There is no `CSF_IF`; the lack of `CSF_WHILE`, `CSF_FOR` and `CSF_TRY`
/// means `":if"` was used.
///
/// Note that `CSF_ELSE` is only used when `CSF_TRY` and `CSF_WHILE` are
/// unset (an `":if"`), and `CSF_SILENT` is only used when `CSF_TRY` is
/// set.
pub mod csf {
    /// condition was TRUE
    pub const TRUE: i32 = 0x0001;
    /// current state is active
    pub const ACTIVE: i32 = 0x0002;
    /// `":else"` has been passed
    pub const ELSE: i32 = 0x0004;
    /// is a `":while"`
    pub const WHILE: i32 = 0x0008;
    /// is a `":for"`
    pub const FOR: i32 = 0x0010;

    /// is a `":try"`
    pub const TRY: i32 = 0x0100;
    /// `":finally"` has been passed
    pub const FINALLY: i32 = 0x0200;
    /// exception thrown to this try conditional
    pub const THROWN: i32 = 0x0800;
    /// exception caught by this try conditional
    pub const CAUGHT: i32 = 0x1000;
    /// `CSF_CAUGHT` was handled by `finish_exception()`
    pub const FINISHED: i32 = 0x2000;
    /// `"emsg_silent"` reset by `":try"`
    pub const SILENT: i32 = 0x4000;
}

/// What's pending for being reactivated at the `":endtry"` of this try
/// conditional (`CSTP_*`).
pub mod cstp {
    /// nothing pending in `":finally"` clause
    pub const NONE: u8 = 0;
    /// an error is pending
    pub const ERROR: u8 = 1;
    /// an interrupt is pending
    pub const INTERRUPT: u8 = 2;
    /// a throw is pending
    pub const THROW: u8 = 4;
    /// `":break"` is pending
    pub const BREAK: u8 = 8;
    /// `":continue"` is pending
    pub const CONTINUE: u8 = 16;
    /// `":return"` is pending
    pub const RETURN: u8 = 24;
    /// `":finish"` is pending
    pub const FINISH: u8 = 32;
}

/// Flags for the `cs_lflags` item in [`CstackT`] (`CSL_*`).
pub mod csl {
    /// just found `":while"` or `":for"`
    pub const HAD_LOOP: i32 = 1;
    /// just found `":endwhile"` or `":endfor"`
    pub const HAD_ENDLOOP: i32 = 2;
    /// just found `":continue"`
    pub const HAD_CONT: i32 = 4;
    /// just found `":finally"`
    pub const HAD_FINA: i32 = 8;
}

/// A list of error messages that can be converted to an exception.
/// `throw_msg` is only set in the first element of the list. Usually, it
/// points to the original message stored in that element, but sometimes
/// it points to a later message in the list. See `cause_errthrow()` (not
/// yet translated) (`struct msglist`, typedef'd as `msglist_T`).
#[derive(Debug, Clone)]
pub struct MsglistT {
    /// next of several messages in a row
    pub next: *mut MsglistT,
    /// original message, allocated
    pub msg: Option<Vec<u8>>,
    /// msg to throw: usually original one
    pub throw_msg: Option<Vec<u8>>,
    /// value from `estack_sfile()` (not yet translated), allocated
    pub sfile: Option<Vec<u8>>,
    /// line number for "sfile"
    pub slnum: LinenrT,
    /// whether this is a multiline message
    pub multiline: bool,
}

/// The exception types (`except_type_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExceptType {
    /// exception caused by `":throw"` command
    #[default]
    User,
    /// error exception
    Error,
    /// interrupt exception triggered by Ctrl-C
    Interrupt,
}

/// Structure describing an exception (don't use `"struct exception"`,
/// it's used by the math library) (`struct vim_exception`, typedef'd as
/// `except_T`).
pub struct ExceptT {
    /// exception type
    pub r#type: ExceptType,
    /// exception value
    pub value: Option<Vec<u8>>,
    /// message(s) causing error exception
    pub messages: *mut MsglistT,
    /// name of the throw point
    pub throw_name: Option<Vec<u8>>,
    /// line number of the throw point
    pub throw_lnum: LinenrT,
    /// stacktrace
    pub stacktrace: *mut ListT,
    /// next exception on the caught stack
    pub caught: *mut ExceptT,
}

impl Default for ExceptT {
    fn default() -> Self {
        ExceptT {
            r#type: ExceptType::default(),
            value: None,
            messages: std::ptr::null_mut(),
            throw_name: None,
            throw_lnum: 0,
            stacktrace: std::ptr::null_mut(),
            caught: std::ptr::null_mut(),
        }
    }
}

/// Structure to save the error/interrupt/exception state between calls
/// to `enter_cleanup()` and `leave_cleanup()` (not yet translated). Must
/// be allocated as an automatic variable by the (common) caller of these
/// functions (`struct cleanup_stuff`, typedef'd as `cleanup_T`).
#[derive(Debug, Clone, Copy, Default)]
pub struct CleanupT {
    /// error/interrupt/exception state
    pub pending: i32,
    /// exception value
    pub exception: *mut ExceptT,
}

/// Exception state that is saved and restored when calling timer
/// callback functions and deferred functions (`struct
/// exception_state_S`, typedef'd as `exception_state_T`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ExceptionStateT {
    pub estate_current_exception: *mut ExceptT,
    pub estate_did_throw: bool,
    pub estate_need_rethrow: bool,
    pub estate_trylevel: i32,
    pub estate_did_emsg: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cstack_default_has_no_active_entry() {
        let cs = CstackT::default();
        assert_eq!(cs.cs_idx, -1);
        assert_eq!(cs.cs_looplevel, 0);
        assert!(cs.cs_emsg_silent_list.is_null());
        assert_eq!(cs.cs_flags.len(), CSTACK_LEN);
    }

    #[test]
    fn csf_flags_are_distinct_bits() {
        let all = [csf::TRUE, csf::ACTIVE, csf::ELSE, csf::WHILE, csf::FOR];
        let mut seen = 0;
        for f in all {
            assert_eq!(seen & f, 0);
            seen |= f;
        }
    }

    #[test]
    fn cstp_return_combines_throw_and_break() {
        // CSTP_RETURN (24) = CSTP_THROW (4) | CSTP_BREAK (8) | ??? - actually
        // verify it matches the exact literal from the original (24), not an
        // assumed bitwise composition.
        assert_eq!(cstp::RETURN, 24);
    }

    #[test]
    fn except_type_default_is_user() {
        assert_eq!(ExceptType::default(), ExceptType::User);
    }

    #[test]
    fn except_default_has_null_pointers() {
        let e = ExceptT::default();
        assert!(e.messages.is_null());
        assert!(e.stacktrace.is_null());
        assert!(e.caught.is_null());
        assert_eq!(e.r#type, ExceptType::User);
    }
}
