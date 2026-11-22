//! Regular expression compilation, execution, and substitution (`src/nvim/regexp.c`).
#![forbid(unsafe_code)]

use crate::buffer::FileBuf;
pub(super) use crate::charset::{ascii_isdigit, ascii_iswhite, ascii_isxdigit};
use crate::charset::{getdigits_int, hex2nr, vim_is_idc, vim_isfilec, vim_isprintc};
use crate::cursor::{Colnr, Linenr, Lpos, lt};
use crate::errors::*;
use crate::eval::eval_to_string;
use crate::eval::typval::{
    TvList, TypVal, VarType, tv_clear, tv_list_alloc, tv_list_alloc_static, tv_list_free_contents,
    tv_list_init_static10, tv_set_string, unref_var_list,
};
use crate::eval::userfunc::{Funcexe, UserFunc};
use crate::globals::*;
use crate::keycodes::{BS, CAR, ESC, K_SPECIAL, NL, NUL, TAB};
use crate::mark::mark_get;
use crate::mbyte::{
    GraphemeState, mb_get_class_tab, mb_islower, mb_isupper, mb_ptr2char_adv, mb_strnicmp,
    mb_tolower, mb_toupper, utf_char2bytes, utf_char2len, utf_composinglike, utf_fold,
    utf_iscomposing_legacy, utf_ptr2char, utf_ptr2len, utf_strnicmp, utfc_ptr2len,
};
use crate::memline::ml_get_buf_len;
use crate::message::{emsg, iemsg, internal_error, msg_puts, semsg, verbose_enter, verbose_leave};
use crate::option_vars::*;
use crate::os::input::fast_breakcheck;
use crate::plines::getvvcol;
use crate::profile::{Proftime, profile_passed_limit};
pub(super) use crate::strings::tr;
use crate::window::WinInfoInfo;
use std::ffi::CStr;

#[path = "regexp_bt.rs"]
mod bt;
#[path = "regexp_nfa.rs"]
mod nfa;

use self::bt::*;
use self::nfa::*;

use crate::option::OptInt;
pub type RegMagic = i32;

pub const NSUBEXP: usize = 10;

pub const RE_MAGIC: i32 = 1;
pub const RE_STRING: i32 = 2;
pub const RE_STRICT: i32 = 4;
pub const RE_AUTO: i32 = 8;
pub const RE_NOBREAK: i32 = 16;

pub const REX_SET: i32 = 1;
pub const REX_USE: i32 = 2;
pub const REX_ALL: i32 = REX_SET | REX_USE;

pub const MAGIC_NONE: RegMagic = 1;
pub const MAGIC_OFF: RegMagic = 2;
pub const MAGIC_ON: RegMagic = 3;
pub const MAGIC_ALL: RegMagic = 4;

pub const REGSUB_COPY: i32 = 1;
pub const REGSUB_MAGIC: i32 = 2;
pub const REGSUB_BACKSLASH: i32 = 4;

pub(super) const CTRL_H_CONST: i32 = 0x08;
pub(super) const CTRL_V_CONST: i32 = 0x16;

pub(super) const MAXCOL: Colnr = 0x7fffffff;
pub(super) const NUMBUFLEN: usize = 65;
pub(super) const MB_MAXBYTES: i32 = 30;
pub(super) const MAX_MCO: usize = 6;
pub(super) const GRAPHEME_STATE_INIT: GraphemeState = 0;
pub(super) const K_MARK_BUF_LOCAL: i32 = 0;
pub(super) const K_CPO_LITERAL: i32 = b'l' as i32;

pub(super) const NFA_MAX_STATES: i32 = 100000;
pub(super) const NFA_TOO_EXPENSIVE: i32 = -1;

pub(super) const AUTOMATIC_ENGINE: i32 = 0;
pub(super) const BACKTRACKING_ENGINE: i32 = 1;
pub(super) const NFA_ENGINE: i32 = 2;

pub(super) const REGMAGIC: i32 = 0o234;
pub(super) const MAX_LIMIT: i32 = 32767 << 16;

pub(super) const NOT_MULTI: i32 = 0;
pub(super) const MULTI_ONE: i32 = 1;
pub(super) const MULTI_MULT: i32 = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RegAction {
    Fail,
    Cont,
    Break,
    Match,
    NoMatch,
}

pub(super) const CLASS_ALNUM: i32 = 0;
pub(super) const CLASS_ALPHA: i32 = 1;
pub(super) const CLASS_BLANK: i32 = 2;
pub(super) const CLASS_CNTRL: i32 = 3;
pub(super) const CLASS_DIGIT: i32 = 4;
pub(super) const CLASS_GRAPH: i32 = 5;
pub(super) const CLASS_LOWER: i32 = 6;
pub(super) const CLASS_PRINT: i32 = 7;
pub(super) const CLASS_PUNCT: i32 = 8;
pub(super) const CLASS_SPACE: i32 = 9;
pub(super) const CLASS_UPPER: i32 = 10;
pub(super) const CLASS_XDIGIT: i32 = 11;
pub(super) const CLASS_TAB: i32 = 12;
pub(super) const CLASS_RETURN: i32 = 13;
pub(super) const CLASS_BACKSPACE: i32 = 14;
pub(super) const CLASS_ESCAPE: i32 = 15;
pub(super) const CLASS_IDENT: i32 = 16;
pub(super) const CLASS_KEYWORD: i32 = 17;
pub(super) const CLASS_FNAME: i32 = 18;
pub(super) const CLASS_NONE: i32 = 99;

pub(super) const RI_DIGIT: i16 = 0x01;
pub(super) const RI_HEX: i16 = 0x02;
pub(super) const RI_OCTAL: i16 = 0x04;
pub(super) const RI_WORD: i16 = 0x08;
pub(super) const RI_HEAD: i16 = 0x10;
pub(super) const RI_ALPHA: i16 = 0x20;
pub(super) const RI_LOWER: i16 = 0x40;
pub(super) const RI_UPPER: i16 = 0x80;
pub(super) const RI_WHITE: i16 = 0x100;

pub(super) const RF_ICASE: u32 = 1;
pub(super) const RF_NOICASE: u32 = 2;
pub(super) const RF_HASNL: u32 = 4;
pub(super) const RF_ICOMBINE: u32 = 8;
pub(super) const RF_LOOKBH: u32 = 16;

pub(super) const REG_NOPAREN: i32 = 0;
pub(super) const REG_PAREN: i32 = 1;
pub(super) const REG_ZPAREN: i32 = 2;
pub(super) const REG_NPAREN: i32 = 3;

pub(super) const REG_MAX_PAREN_DEPTH: i32 = 1000;
pub(super) const MAX_REGSUB_NESTING: usize = 4;

pub(super) const CLASS_NOT: i32 = 0x80;
pub(super) const CLASS_AF_CONST: i32 = 0x40;
pub(super) const CLASS_AF: i32 = 0x20;
pub(super) const CLASS_AZ_CONST: i32 = 0x10;
pub(super) const CLASS_AZ: i32 = 0x08;
pub(super) const CLASS_O7: i32 = 0x04;
pub(super) const CLASS_O9: i32 = 0x02;
pub(super) const CLASS_UNDERSCORE: i32 = 0x01;

pub(super) const REGEXP_INRANGE: &CStr = c"]^-n\\";
pub(super) const REGEXP_ABBR: &CStr = c"nrtebdoxuU";
pub(super) const CLASSCHARS: &CStr = c".iIkKfFpPsSdDxXoOwWhHaAlLuU";

pub(super) const E_INVALID_CHARACTER_AFTER_STR_AT: &CStr = c"E59: Invalid character after %s@";
pub(super) const E_INVALID_USE_OF_UNDERSCORE: &CStr = c"E63: Invalid use of \\_";
pub(super) const E_PATTERN_USES_MORE_MEMORY_THAN_MAXMEMPATTERN: &CStr =
    c"E363: Pattern uses more memory than 'maxmempattern'";
pub(super) const E_INVALID_ITEM_IN_STR_BRACKETS: &CStr = c"E369: Invalid item in %s%%[]";
pub(super) const E_MISSING_DELIMITER_AFTER_SEARCH_PATTERN_STR: &CStr =
    c"E654: Missing delimiter after search pattern: %s";
pub(super) const E_MISSINGBRACKET: &CStr = c"E769: Missing ] after %s[";
pub(super) const E_REVERSE_RANGE: &CStr = c"E944: Reverse range in character class";
pub(super) const E_LARGE_CLASS: &CStr = c"E945: Range too large in character class";
pub(super) const E_UNMATCHED_Z: &CStr = c"E52: Unmatched \\z(";
pub(super) const E_UNMATCHED_STR_PERCENT_OPEN: &CStr = c"E53: Unmatched %s%%(";
pub(super) const E_UNMATCHED_STR_OPEN: &CStr = c"E54: Unmatched %s(";
pub(super) const E_UNMATCHED_STR_CLOSE: &CStr = c"E55: Unmatched %s)";
pub(super) const E_Z_NOT_ALLOWED: &CStr = c"E66: \\z( not allowed here";
pub(super) const E_Z1_NOT_ALLOWED: &CStr = c"E67: \\z1 - \\z9 not allowed here";
pub(super) const E_MISSING_SB: &CStr = c"E69: Missing ] after %s%%[";
pub(super) const E_EMPTY_SB: &CStr = c"E70: Empty %s%%[]";
pub(super) const E_RECURSIVE: &CStr = c"E956: Cannot use pattern recursively";
pub(super) const E_REGEXP_NUMBER_AFTER_DOT_POS_SEARCH_CHR: &CStr =
    c"E1204: No Number allowed after .: '\\%%%c'";
pub(super) const E_NFA_REGEXP_MISSING_VALUE_IN_CHR: &CStr =
    c"E1273: (NFA regexp) missing value in '\\%%%c'";
pub(super) const E_ATOM_ENGINE_MUST_BE_AT_START_OF_PATTERN: &CStr =
    c"E1281: Atom '\\%%#=%c' must be at the start of the pattern";
pub(super) const E_SUBSTITUTE_NESTING_TOO_DEEP: &CStr = c"E1290: substitute nesting too deep";
pub(super) const E_UNICODE_VAL_TOO_LARGE: &CStr =
    c"E1541: Value too large, max Unicode codepoint is U+10FFFF";

#[rustfmt::skip]
pub(super) static META_FLAGS: [u8; 127] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//                 %  &     (  )  *  +        .
    0, 0, 0, 0, 0, 1, 1, 0, 1, 1, 1, 1, 0, 0, 1, 0,
//     1  2  3  4  5  6  7  8  9        <  =  >  ?
    0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1,
//  @  A     C  D     F     H  I     K  L  M     O
    1, 1, 0, 1, 1, 0, 1, 0, 1, 1, 0, 1, 1, 1, 0, 1,
//  P        S     U  V  W  X     Z  [           _
    1, 0, 0, 1, 0, 1, 1, 1, 1, 0, 1, 1, 0, 0, 0, 1,
//     a     c  d     f     h  i     k  l  m  n  o
    0, 1, 0, 1, 1, 0, 1, 0, 1, 1, 0, 1, 1, 1, 1, 1,
//  p        s     u  v  w  x     z  {  |     ~
    1, 0, 0, 1, 0, 1, 1, 1, 1, 0, 1, 1, 1, 0, 1,
];

pub(super) enum RegProgEngine {
    Bt(BtRegprog),
    Nfa(NfaRegprog),
}

pub struct RegprogInner {
    pub regflags: std::cell::Cell<u32>,
    pub re_engine: std::cell::Cell<u32>,
    pub re_flags: u32,
    pub re_in_use: std::cell::Cell<bool>,
    pub(super) engine: std::cell::RefCell<RegProgEngine>,
}

#[derive(Clone)]
pub struct Regprog(std::rc::Rc<RegprogInner>);

impl Regprog {
    pub(super) fn new(regflags: u32, re_engine: u32, re_flags: u32, engine: RegProgEngine) -> Self {
        Self(std::rc::Rc::new(RegprogInner {
            regflags: std::cell::Cell::new(regflags),
            re_engine: std::cell::Cell::new(re_engine),
            re_flags,
            re_in_use: std::cell::Cell::new(false),
            engine: std::cell::RefCell::new(engine),
        }))
    }

    #[inline(always)]
    pub fn regflags(&self) -> u32 {
        self.0.regflags.get()
    }

    #[inline(always)]
    pub fn re_engine(&self) -> u32 {
        self.0.re_engine.get()
    }

    #[inline(always)]
    pub fn re_flags(&self) -> u32 {
        self.0.re_flags
    }

    #[inline(always)]
    pub fn re_in_use(&self) -> bool {
        self.0.re_in_use.get()
    }

    #[inline(always)]
    pub fn set_re_in_use(&self, val: bool) {
        self.0.re_in_use.set(val);
    }

    #[inline(always)]
    pub fn replace_from(&self, other: &Self) {
        self.0.regflags.set(other.regflags());
        self.0.re_engine.set(other.re_engine());
        let new_eng = match &*other.0.engine.borrow() {
            RegProgEngine::Bt(bt) => RegProgEngine::Bt(bt.clone()),
            RegProgEngine::Nfa(nfa) => RegProgEngine::Nfa(nfa.clone()),
        };
        *self.0.engine.borrow_mut() = new_eng;
    }

    #[inline(always)]
    pub fn nfa_pattern(&self) -> std::ffi::CString {
        match &*self.0.engine.borrow() {
            RegProgEngine::Nfa(nfa) => nfa.pattern.clone(),
            RegProgEngine::Bt(_) => unreachable!(),
        }
    }

    #[inline(always)]
    pub fn bt_magic_wrong(&self) -> bool {
        match &*self.0.engine.borrow() {
            RegProgEngine::Bt(bt) => bt.program.first().copied() != Some(REGMAGIC as u8),
            RegProgEngine::Nfa(_) => false,
        }
    }
}

#[derive(Clone, Default)]
pub struct RegprogPtr(Option<Regprog>);

impl RegprogPtr {
    pub const NULL: Self = Self(None);

    #[inline(always)]
    pub const fn is_null(&self) -> bool {
        self.0.is_none()
    }

    #[inline(always)]
    pub fn opt(&self) -> Option<&Regprog> {
        self.0.as_ref()
    }

    #[inline(always)]
    pub fn from_box(prog: Option<Box<Regprog>>) -> Self {
        Self(prog.map(|b| *b))
    }

    #[inline(always)]
    pub fn from_ref(prog: Option<&Regprog>) -> Self {
        Self(prog.cloned())
    }
}

impl std::ops::Deref for RegprogPtr {
    type Target = Regprog;

    #[inline(always)]
    fn deref(&self) -> &Regprog {
        self.0.as_ref().unwrap()
    }
}

impl PartialEq for RegprogPtr {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => std::rc::Rc::ptr_eq(&a.0, &b.0),
            (None, None) => true,
            _ => false,
        }
    }
}

impl Eq for RegprogPtr {}

/// The result of matching lines of a buffer. `startpos[i]` and `endpos[i]` are where group `i`
/// (0 is the whole match) starts and ends, with line numbers counted from the line where matching
/// started, or `None` when the group did not match.
#[derive(Clone)]
pub struct Regmmatch {
    pub regprog: RegprogPtr,
    pub startpos: [Option<Lpos>; NSUBEXP],
    pub endpos: [Option<Lpos>; NSUBEXP],
    pub rmm_matchcol: Colnr,
    pub rmm_ic: bool,
    pub rmm_maxcol: Colnr,
}

impl Regmmatch {
    pub const fn new() -> Self {
        Self {
            regprog: RegprogPtr::NULL,
            startpos: [None; NSUBEXP],
            endpos: [None; NSUBEXP],
            rmm_matchcol: 0,
            rmm_ic: false,
            rmm_maxcol: 0,
        }
    }

    /// Where a successful match starts.
    #[inline(always)]
    pub fn match_start(&self) -> Lpos {
        self.startpos[0].expect("a successful match sets its start")
    }

    /// Where a successful match ends. A `\ze` before `\zs` can put it before the start.
    #[inline(always)]
    pub fn match_end(&self) -> Lpos {
        self.endpos[0].expect("a successful match sets its end")
    }

    #[inline(always)]
    pub fn set_prog(&mut self, prog: Option<Box<Regprog>>) {
        self.regprog = RegprogPtr::from_box(prog);
    }

    #[inline(always)]
    pub fn borrow_prog(&mut self, prog: Option<&Regprog>) {
        self.regprog = RegprogPtr::from_ref(prog);
    }

    #[inline(always)]
    pub fn free_prog(&mut self) {
        self.regprog = RegprogPtr::NULL;
    }
}

impl Default for Regmmatch {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

/// A line that a match or a substitution reads, without its NUL: the matched string, or a
/// line of the buffer being searched.
#[derive(Clone)]
pub(super) enum RegLine<'a> {
    Str(&'a [u8]),
    Buf(crate::option_vars::RcCStr),
}

impl RegLine<'_> {
    #[inline(always)]
    pub(super) fn bytes(&self) -> &[u8] {
        match self {
            Self::Str(s) => s,
            Self::Buf(s) => s.to_bytes(),
        }
    }
}

impl Default for RegLine<'_> {
    fn default() -> Self {
        Self::Str(b"")
    }
}

/// The result of matching one string. `startp[i]` and `endp[i]` are the byte offsets of group
/// `i` (0 is the whole match) in the matched string, or `None` when the group did not match.
#[derive(Clone)]
pub struct Regmatch {
    pub regprog: RegprogPtr,
    pub startp: [Option<usize>; NSUBEXP],
    pub endp: [Option<usize>; NSUBEXP],
    pub rm_matchcol: Colnr,
    pub rm_ic: bool,
}

impl Regmatch {
    pub const fn new() -> Self {
        Self {
            regprog: RegprogPtr::NULL,
            startp: [None; NSUBEXP],
            endp: [None; NSUBEXP],
            rm_matchcol: 0,
            rm_ic: false,
        }
    }

    /// Byte offset where a successful match starts.
    #[inline(always)]
    pub fn match_start(&self) -> usize {
        self.startp[0].expect("a successful match sets its start")
    }

    /// Byte offset where a successful match ends. A `\ze` before `\zs` can put it before the
    /// start.
    #[inline(always)]
    pub fn match_end(&self) -> usize {
        self.endp[0].expect("a successful match sets its end")
    }

    /// The text of a successful match in `subject`, the string that was matched.
    #[inline(always)]
    pub fn matched_text<'s>(&self, subject: &'s [u8]) -> &'s [u8] {
        self.group(0, subject)
            .expect("a successful match sets its start and end")
    }

    /// The text group `idx` matched in `subject`, the string that was matched; `None` when the
    /// group did not match. Empty when `\ze` put its end before its start.
    #[inline(always)]
    pub fn group<'s>(&self, idx: usize, subject: &'s [u8]) -> Option<&'s [u8]> {
        let start = self.startp[idx]?.min(subject.len());
        let end = self.endp[idx]?.clamp(start, subject.len());
        Some(&subject[start..end])
    }

    /// `subject`, the string that was matched, from where group `idx` starts; `None` when the
    /// group did not match.
    #[inline(always)]
    pub fn text_from<'s>(&self, idx: usize, subject: &'s [u8]) -> Option<&'s [u8]> {
        Some(&subject[self.startp[idx]?.min(subject.len())..])
    }

    #[inline(always)]
    pub fn set_prog(&mut self, prog: Option<Box<Regprog>>) {
        self.regprog = RegprogPtr::from_box(prog);
    }

    #[inline(always)]
    pub fn borrow_prog(&mut self, prog: Option<&Regprog>) {
        self.regprog = RegprogPtr::from_ref(prog);
    }

    #[inline(always)]
    pub fn free_prog(&mut self) {
        self.regprog = RegprogPtr::NULL;
    }
}

impl Default for Regmatch {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
pub struct RegExtmatch {
    pub matches: [Option<std::ffi::CString>; NSUBEXP],
}

#[inline(always)]
pub(super) fn cstring_from_bytes(bytes: &[u8]) -> Option<std::ffi::CString> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::ffi::CString::new(&bytes[..end]).ok()
}

#[derive(Clone)]
pub(super) struct BtRegprog {
    pub regflags: u32,
    pub regstart: i32,
    pub regstart_class: i16,
    pub reganch: u8,
    pub regmust_off: Option<usize>,
    pub regmlen: i32,
    pub reghasz: u8,
    pub program: Vec<u8>,
}

#[derive(Clone, Copy)]
pub(super) enum NfaOutSlot {
    Out(usize),
    Out1(usize),
}

impl NfaOutSlot {
    #[inline(always)]
    pub(super) fn set(self, states: &mut [NfaState], target: Option<usize>) {
        match self {
            Self::Out(idx) => states[idx].out = target,
            Self::Out1(idx) => states[idx].out1 = target,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct NfaState {
    pub c: i32,
    pub out: Option<usize>,
    pub out1: Option<usize>,
    pub val: i32,
}

impl NfaState {
    pub(super) const fn new() -> Self {
        Self {
            c: 0,
            out: None,
            out1: None,
            val: 0,
        }
    }
}

impl Default for NfaState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub(super) struct NfaRegprog {
    pub regflags: u32,
    pub re_engine: u32,
    pub start: Option<usize>,
    pub reganch: bool,
    pub regstart: i32,
    pub match_text: Option<std::ffi::CString>,
    pub has_zend: bool,
    pub has_backref: bool,
    pub reghasz: i32,
    pub pattern: std::ffi::CString,
    pub nsubexp: i32,
    pub nstate: i32,
    pub state: Vec<NfaState>,
}

/// A position in what a match reads: a byte offset in the matched string (`None` for upstream's
/// NULL), or a line and column when matching buffer lines.
#[derive(Clone, Copy)]
pub(super) struct RegPos {
    pub ptr: Option<usize>,
    pub pos: Lpos,
}

impl RegPos {
    pub(super) const fn new() -> Self {
        Self {
            ptr: None,
            pos: Lpos::new(),
        }
    }

    #[inline(always)]
    pub(super) const fn from_pos(pos: Lpos) -> Self {
        Self { ptr: None, pos }
    }

    #[inline(always)]
    pub(super) const fn pos(self) -> Lpos {
        self.pos
    }

    #[inline(always)]
    pub(super) const fn pos_mut(&mut self) -> &mut Lpos {
        &mut self.pos
    }
}

impl Default for RegPos {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Regsave {
    pub rs_u: RegPos,
    pub rs_len: i32,
}

impl Regsave {
    pub(super) const fn new() -> Self {
        Self {
            rs_u: RegPos::new(),
            rs_len: 0,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct SaveSe {
    pub se_u: RegPos,
}

impl SaveSe {
    pub(super) const fn new() -> Self {
        Self {
            se_u: RegPos::new(),
        }
    }
}

pub(super) type Regstate = i32;
pub(super) const RS_NOPEN: Regstate = 0;
pub(super) const RS_MOPEN: Regstate = 1;
pub(super) const RS_MCLOSE: Regstate = 2;
pub(super) const RS_ZOPEN: Regstate = 3;
pub(super) const RS_ZCLOSE: Regstate = 4;
pub(super) const RS_BRANCH: Regstate = 5;
pub(super) const RS_BRCPLX_MORE: Regstate = 6;
pub(super) const RS_BRCPLX_LONG: Regstate = 7;
pub(super) const RS_BRCPLX_SHORT: Regstate = 8;
pub(super) const RS_NOMATCH: Regstate = 9;
pub(super) const RS_BEHIND1: Regstate = 10;
pub(super) const RS_BEHIND2: Regstate = 11;
pub(super) const RS_STAR_LONG: Regstate = 12;
pub(super) const RS_STAR_SHORT: Regstate = 13;
/// Left by a `\@=` or `\@>` whose operand changed groups: undoes those changes when matching backs
/// out past it.
pub(super) const RS_RESTORE: Regstate = 14;

#[derive(Clone, Copy)]
pub(super) enum RegItemSave {
    Se(SaveSe),
    Reg(Regsave),
}

impl RegItemSave {
    pub(super) const fn new() -> Self {
        Self::Se(SaveSe::new())
    }

    #[inline(always)]
    pub(super) fn sesave(&self) -> &SaveSe {
        match self {
            Self::Se(s) => s,
            Self::Reg(_) => unreachable!(),
        }
    }

    #[inline(always)]
    pub(super) fn sesave_mut(&mut self) -> &mut SaveSe {
        if !matches!(*self, Self::Se(_)) {
            *self = Self::Se(SaveSe::new());
        }
        match self {
            Self::Se(s) => s,
            Self::Reg(_) => unreachable!(),
        }
    }

    #[inline(always)]
    pub(super) fn regsave(&self) -> &Regsave {
        match self {
            Self::Reg(r) => r,
            Self::Se(_) => unreachable!(),
        }
    }

    #[inline(always)]
    pub(super) fn regsave_mut(&mut self) -> &mut Regsave {
        if !matches!(*self, Self::Reg(_)) {
            *self = Self::Reg(Regsave::new());
        }
        match self {
            Self::Reg(r) => r,
            Self::Se(_) => unreachable!(),
        }
    }
}

impl Default for RegItemSave {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
pub(super) struct Regitem {
    pub rs_state: Regstate,
    pub rs_no: i16,
    pub rs_scan: Option<usize>,
    pub rs_un: RegItemSave,
}

impl Regitem {
    pub(super) const fn new() -> Self {
        Self {
            rs_state: 0,
            rs_no: 0,
            rs_scan: None,
            rs_un: RegItemSave::new(),
        }
    }
}

impl Default for Regitem {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Regbehind {
    pub save_after: Regsave,
    pub save_behind: Regsave,
    /// How many characters the start of the look-behind lies before its end, or before the end
    /// of the previous line once it crossed there.
    pub chars_back: i32,
    /// How long the group trail was when the look-behind started: undoing it back to there puts
    /// back the groups that the look-behind and what follows it set.
    pub trail_len: usize,
}

pub(super) struct FragOut {
    inline: [NfaOutSlot; 2],
    len: usize,
    overflow: Vec<NfaOutSlot>,
}

impl FragOut {
    #[inline(always)]
    pub(super) const fn new() -> Self {
        Self {
            inline: [NfaOutSlot::Out(0); 2],
            len: 0,
            overflow: Vec::new(),
        }
    }

    #[inline(always)]
    pub(super) const fn from_one(slot: NfaOutSlot) -> Self {
        Self {
            inline: [slot, NfaOutSlot::Out(0)],
            len: 1,
            overflow: Vec::new(),
        }
    }

    #[inline(always)]
    pub(super) const fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline(always)]
    pub(super) fn push(&mut self, slot: NfaOutSlot) {
        if self.len < 2 {
            self.inline[self.len] = slot;
        } else {
            self.overflow.push(slot);
        }
        self.len += 1;
    }

    #[inline(always)]
    pub(super) fn extend(&mut self, other: Self) {
        for &slot in &other.inline[..other.len.min(2)] {
            self.push(slot);
        }
        if !other.overflow.is_empty() {
            if self.len < 2 {
                for slot in other.overflow {
                    self.push(slot);
                }
            } else {
                self.len += other.overflow.len();
                self.overflow.extend(other.overflow);
            }
        }
    }

    #[inline(always)]
    pub(super) fn patch(self, states: &mut [NfaState], target: Option<usize>) {
        for &slot in &self.inline[..self.len.min(2)] {
            slot.set(states, target);
        }
        for slot in self.overflow {
            slot.set(states, target);
        }
    }
}

pub(super) struct Frag {
    pub start: Option<usize>,
    pub out: FragOut,
}

impl Frag {
    pub(super) const fn new() -> Self {
        Self {
            start: None,
            out: FragOut::new(),
        }
    }
}

impl Default for Frag {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct MultiPos {
    pub start_lnum: Linenr,
    pub end_lnum: Linenr,
    pub start_col: Colnr,
    pub end_col: Colnr,
}

impl MultiPos {
    #[inline(always)]
    pub(super) const fn new() -> Self {
        Self {
            start_lnum: 0,
            end_lnum: 0,
            start_col: 0,
            end_col: 0,
        }
    }

    #[inline(always)]
    pub(super) const fn empty() -> Self {
        Self {
            start_lnum: -1,
            end_lnum: -1,
            start_col: -1,
            end_col: -1,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Regsub {
    pub in_use: i32,
    pub orig_start_col: Colnr,
    pub list: [MultiPos; NSUBEXP],
}

impl Regsub {
    #[inline(always)]
    pub(super) const fn new() -> Self {
        Self {
            in_use: 0,
            orig_start_col: 0,
            list: [MultiPos::new(); NSUBEXP],
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Regsubs {
    pub norm: Regsub,
    pub synt: Regsub,
}

impl Regsubs {
    #[inline(always)]
    pub(super) const fn new() -> Self {
        Self {
            norm: Regsub::new(),
            synt: Regsub::new(),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct NfaPim {
    pub result: i32,
    pub state: Option<usize>,
    pub subs: Regsubs,
    pub end: RegPos,
}

impl NfaPim {
    #[inline(always)]
    pub(super) const fn new() -> Self {
        Self {
            result: 0,
            state: None,
            subs: Regsubs::new(),
            end: RegPos::new(),
        }
    }
}

impl Default for NfaPim {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
pub(super) struct NfaThread {
    pub state: Option<usize>,
    pub count: i32,
    pub pim: NfaPim,
    pub subs: Regsubs,
}

impl NfaThread {
    #[inline(always)]
    pub(super) const fn new() -> Self {
        Self {
            state: None,
            count: 0,
            pim: NfaPim::new(),
            subs: Regsubs::new(),
        }
    }
}

impl Default for NfaThread {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) struct NfaList {
    pub t: Vec<NfaThread>,
    pub n: i32,
    pub id: i32,
    pub has_pim: bool,
    /// While addstate_here() inserts at a thread, the end of the threads still queued
    /// behind it; zero otherwise.
    pub queued_end: usize,
}

#[derive(Clone, Copy)]
enum CaseConv {
    Upper,
    Lower,
}

impl CaseConv {
    #[inline(always)]
    fn apply(self, c: i32) -> i32 {
        match self {
            Self::Upper => mb_toupper(c),
            Self::Lower => mb_tolower(c),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ParseState {
    pub regparse_off: usize,
    pub prevchr_len: i32,
    pub curchr: i32,
    pub prevchr: i32,
    pub prevprevchr: i32,
    pub nextchr: i32,
    pub at_start: i32,
    pub prev_at_start: i32,
    pub regnpar: i32,
}

impl ParseState {
    pub(super) const fn new() -> Self {
        Self {
            regparse_off: 0,
            prevchr_len: 0,
            curchr: 0,
            prevchr: 0,
            prevprevchr: 0,
            nextchr: 0,
            at_start: 0,
            prev_at_start: 0,
            regnpar: 0,
        }
    }
}

impl Default for ParseState {
    fn default() -> Self {
        Self::new()
    }
}

/// What a match reads.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    /// One string (upstream's `rex.reg_match`).
    Str,
    /// Lines of a buffer (upstream's `rex.reg_mmatch`).
    Lines,
}

/// One match: what it reads, where it is, and the group positions it has found (upstream's
/// `rex`). A substitution reads a finished match through one as well.
pub(super) struct Regexec<'a> {
    target: Target,
    bt_magic_wrong: bool,
    rm_startp: [Option<usize>; NSUBEXP],
    rm_endp: [Option<usize>; NSUBEXP],
    rm_matchcol: Colnr,
    rmm_startpos: [Lpos; NSUBEXP],
    rmm_endpos: [Lpos; NSUBEXP],
    rmm_matchcol: Colnr,
    reg_win: crate::globals::WinPtr,
    reg_win_cursor: Option<crate::cursor::CursorPos>,
    reg_buf: crate::globals::BufPtr,
    reg_chartab: [u64; 4],
    reg_firstlnum: Linenr,
    reg_maxline: Linenr,
    reg_line_lbr: bool,
    lnum: Linenr,
    line: RegLine<'a>,
    /// Byte offset of the current position in `line`.
    input: usize,
    need_clear_subexpr: bool,
    need_clear_zsubexpr: bool,
    reg_ic: bool,
    reg_icombine: bool,
    reg_nobreak: bool,
    reg_maxcol: Colnr,
    reg_startzp: [Option<usize>; NSUBEXP],
    reg_endzp: [Option<usize>; NSUBEXP],
    reg_startzpos: [Lpos; NSUBEXP],
    reg_endzpos: [Lpos; NSUBEXP],
    brace_min: [i64; 10],
    brace_max: [i64; 10],
    brace_count: [i32; 10],
    bl_minval: i64,
    bl_maxval: i64,
    behind_pos: Regsave,
    nfa_has_zend: bool,
    nfa_has_backref: bool,
    nfa_nsubexpr: i32,
    nfa_listid: i32,
    nfa_alt_listid: i32,
    nfa_has_zsubexpr: bool,
    nfa_endp: Option<SaveSe>,
    nfa_ll_index: usize,
    nfa_match: i32,
    nfa_time_limit: Option<Proftime>,
    nfa_timed_out: bool,
    nfa_time_count: i32,
}

impl<'a> Regexec<'a> {
    pub(super) fn new(target: Target, line: RegLine<'a>) -> Self {
        Self {
            target,
            bt_magic_wrong: false,
            rm_startp: [None; NSUBEXP],
            rm_endp: [None; NSUBEXP],
            rm_matchcol: 0,
            rmm_startpos: [Lpos::new(); NSUBEXP],
            rmm_endpos: [Lpos::new(); NSUBEXP],
            rmm_matchcol: 0,
            reg_win: crate::globals::WinPtr::NULL,
            reg_win_cursor: None,
            reg_buf: crate::globals::BufPtr::NULL,
            reg_chartab: [0; 4],
            reg_firstlnum: 0,
            reg_maxline: 0,
            reg_line_lbr: false,
            lnum: 0,
            line,
            input: 0,
            need_clear_subexpr: false,
            need_clear_zsubexpr: false,
            reg_ic: false,
            reg_icombine: false,
            reg_nobreak: false,
            reg_maxcol: 0,
            reg_startzp: [None; NSUBEXP],
            reg_endzp: [None; NSUBEXP],
            reg_startzpos: [Lpos::new(); NSUBEXP],
            reg_endzpos: [Lpos::new(); NSUBEXP],
            brace_min: [0; 10],
            brace_max: [0; 10],
            brace_count: [0; 10],
            bl_minval: 0,
            bl_maxval: 0,
            behind_pos: Regsave::new(),
            nfa_has_zend: false,
            nfa_has_backref: false,
            nfa_nsubexpr: 0,
            nfa_listid: 0,
            nfa_alt_listid: 0,
            nfa_has_zsubexpr: false,
            nfa_endp: None,
            nfa_ll_index: 0,
            nfa_match: 0,
            nfa_time_limit: None,
            nfa_timed_out: false,
            nfa_time_count: 0,
        }
    }

    /// Prepares matching `line`, a string without NUL; `nl` makes a "\n" in it match a line break.
    pub(super) fn for_string(rmp: &Regmatch, line: &'a [u8], nl: bool) -> Self {
        debug_assert!(!line.contains(&0), "a matched string ends before its NUL");
        let mut rex = Self::new(Target::Str, RegLine::Str(line));
        rex.rm_matchcol = rmp.rm_matchcol;
        rex.reg_line_lbr = nl;
        rex.set_buf(workspace_state().curbuf.get());
        rex.reg_ic = rmp.rm_ic;
        rex.reg_nobreak = (rmp.regprog.re_flags() & (RE_NOBREAK as u32)) != 0;
        rex
    }

    /// Prepares matching lines of `buf` from `lnum`, shown in `win` when it is given.
    pub(super) fn for_lines(
        rmp: &Regmmatch,
        win: Option<&WinInfoInfo>,
        buf: &FileBuf,
        lnum: Linenr,
    ) -> Self {
        let mut rex = Self::new(Target::Lines, RegLine::default());
        rex.rmm_matchcol = rmp.rmm_matchcol;
        rex.set_buf(crate::globals::BufPtr::from(buf));
        rex.reg_win = win.map_or(crate::globals::WinPtr::NULL, crate::globals::WinPtr::from);
        rex.reg_win_cursor = win.map(|wp| wp.w_cursor.get());
        rex.reg_firstlnum = lnum;
        rex.reg_maxline = buf.b_ml.ml_line_count.get() - lnum;
        rex.reg_ic = rmp.rmm_ic;
        rex.reg_nobreak = (rmp.regprog.re_flags() & (RE_NOBREAK as u32)) != 0;
        rex.reg_maxcol = rmp.rmm_maxcol;
        rex
    }

    #[inline(always)]
    pub(super) fn set_buf(&mut self, buf: crate::globals::BufPtr) {
        if let Some(b) = buf.opt() {
            self.reg_chartab = b.b_chartab.get();
        }
        self.reg_buf = buf;
    }

    /// Matching buffer lines (upstream's `REG_MULTI`).
    #[inline(always)]
    pub(super) fn reg_multi(&self) -> bool {
        self.target == Target::Lines
    }

    /// The current line.
    #[inline(always)]
    pub(super) fn line(&self) -> &[u8] {
        self.line.bytes()
    }

    /// The current line from the current position.
    #[inline(always)]
    pub(super) fn rest(&self) -> &[u8] {
        self.line_from(self.input)
    }

    /// The current line from byte offset `col`.
    #[inline(always)]
    pub(super) fn line_from(&self, col: usize) -> &[u8] {
        let line = self.line();
        &line[col.min(line.len())..]
    }

    /// The byte at the current position; NUL at the end of the line.
    #[inline(always)]
    pub(super) fn input_byte(&self) -> u8 {
        self.line().get(self.input).copied().unwrap_or(0)
    }

    #[inline(always)]
    pub(super) fn input_col(&self) -> Colnr {
        self.input as Colnr
    }

    /// The current position: a line counted from the first line of the match, and a column.
    #[inline(always)]
    pub(super) fn cur_pos(&self) -> Lpos {
        Lpos {
            lnum: self.lnum,
            col: self.input_col(),
        }
    }

    /// Moves the current position `n` bytes forward, but not past the end of the line.
    #[inline(always)]
    pub(super) fn advance_input(&mut self, n: usize) {
        self.input = (self.input + n).min(self.line().len());
    }

    /// Sets the current position to byte offset `col`, but not past the end of the line.
    #[inline(always)]
    pub(super) fn set_input(&mut self, col: usize) {
        self.input = col.min(self.line().len());
    }

    /// Line `lnum` counted from the first line of the match: `None` above the first line of the
    /// buffer, and empty below the last line the match may read.
    pub(super) fn getline(&self, lnum: Linenr) -> Option<RegLine<'a>> {
        let buf_lnum = self.reg_firstlnum + lnum;
        if buf_lnum < 1 {
            return None;
        }
        if lnum > self.reg_maxline {
            return Some(RegLine::Str(b""));
        }
        Some(RegLine::Buf(crate::memline::ml_get_buf(
            &self.reg_buf,
            buf_lnum,
        )))
    }

    /// The length of line `lnum`, counted as for [`Regexec::getline`].
    pub(super) fn getline_len(&self, lnum: Linenr) -> Colnr {
        let buf_lnum = self.reg_firstlnum + lnum;
        if buf_lnum < 1 || lnum > self.reg_maxline {
            return 0;
        }
        ml_get_buf_len(&self.reg_buf, buf_lnum)
    }

    /// Moves to the start of the next line.
    pub(super) fn nextline(&mut self) {
        self.lnum += 1;
        self.line = self.getline(self.lnum).unwrap_or(RegLine::Str(b""));
        self.input = 0;
        self.breakcheck();
    }

    #[inline(always)]
    pub(super) fn breakcheck(&self) {
        if !self.reg_nobreak {
            fast_breakcheck();
        }
    }

    pub(super) fn iswordc(&self, c: i32) -> bool {
        crate::charset::vim_iswordc_tab(c, &self.reg_chartab)
    }

    /// The column of the character before column `col` of the current line.
    pub(super) fn prev_char_col(&self, col: Colnr) -> Colnr {
        if col <= 0 {
            return 0;
        }
        let line = self.line();
        let prev = (col - 1) as usize;
        if prev >= line.len() {
            return prev.min(line.len()) as Colnr;
        }
        (prev - crate::mbyte::utf_head_off_slice(line, prev) as usize) as Colnr
    }

    /// The column up to `n` characters before column `col` of the current line, stopping at its
    /// start, and how many characters that is.
    pub(super) fn chars_before(&self, mut col: Colnr, n: i32) -> (Colnr, i32) {
        let start = (col - n).max(0);
        // Each ASCII byte is a character.
        let window = self.line().get(start as usize..col as usize);
        if window.is_some_and(<[u8]>::is_ascii) {
            return (start, col - start);
        }
        let mut back = 0;
        while back < n && col > 0 {
            col = self.prev_char_col(col);
            back += 1;
        }
        (col, back)
    }

    /// The class of the character before the current position, -1 at the start of the line.
    pub(super) fn prev_class(&self) -> i32 {
        if self.input == 0 {
            return -1;
        }
        let line = self.line();
        let prev = (self.input - 1).min(line.len().saturating_sub(1));
        let head = prev - crate::mbyte::utf_head_off_slice(line, prev) as usize;
        mb_get_class_tab(&line[head..], &self.reg_chartab)
    }

    pub(super) fn prog_magic_wrong(&self) -> bool {
        if self.bt_magic_wrong {
            emsg(tr(E_RE_CORR));
            return true;
        }
        false
    }

    pub(super) fn cleanup_subexpr(&mut self) {
        if !self.need_clear_subexpr {
            return;
        }
        if self.reg_multi() {
            self.rmm_startpos = [UNSET_LPOS; NSUBEXP];
            self.rmm_endpos = [UNSET_LPOS; NSUBEXP];
        } else {
            self.rm_startp = [None; NSUBEXP];
            self.rm_endp = [None; NSUBEXP];
        }
        self.need_clear_subexpr = false;
    }

    pub(super) fn cleanup_zsubexpr(&mut self) {
        if !self.need_clear_zsubexpr {
            return;
        }
        if self.reg_multi() {
            self.reg_startzpos = [UNSET_LPOS; NSUBEXP];
            self.reg_endzpos = [UNSET_LPOS; NSUBEXP];
        } else {
            self.reg_startzp = [None; NSUBEXP];
            self.reg_endzp = [None; NSUBEXP];
        }
        self.need_clear_zsubexpr = false;
    }

    #[inline(always)]
    pub(super) fn save_match(&self, rmp: &mut Regmatch) {
        rmp.startp = self.rm_startp;
        rmp.endp = self.rm_endp;
        rmp.rm_matchcol = self.rm_matchcol;
    }

    #[inline(always)]
    pub(super) fn save_mmatch(&self, rmp: &mut Regmmatch) {
        rmp.startpos = self.rmm_startpos.map(matched_pos);
        rmp.endpos = self.rmm_endpos.map(matched_pos);
        rmp.rmm_matchcol = self.rmm_matchcol;
    }
}

/// How the engines mark the position of a group in buffer lines that is not set: line -1, as
/// upstream does. Matches leave the engines with such positions as `None`.
pub(super) const UNSET_LPOS: Lpos = Lpos { lnum: -1, col: -1 };

/// A group position from the engines, or `None` when it is not set.
#[inline(always)]
fn matched_pos(pos: Lpos) -> Option<Lpos> {
    (pos.lnum >= 0).then_some(pos)
}

/// The groups of a match in a string, kept while a substitution evaluates an expression: the
/// text they cover, which starts at offset `text_off` of the string, and their positions in it.
#[derive(Clone)]
pub(super) struct SubmatchText {
    text: std::rc::Rc<[u8]>,
    text_off: usize,
    startp: [Option<usize>; NSUBEXP],
    endp: [Option<usize>; NSUBEXP],
}

impl SubmatchText {
    /// The text group `no` matched; `None` when it did not match.
    fn group(&self, no: usize) -> Option<&[u8]> {
        let (start, end) = (self.startp[no]?, self.endp[no]?);
        let from = start.saturating_sub(self.text_off).min(self.text.len());
        Some(&self.text[from..(from + end.saturating_sub(start)).min(self.text.len())])
    }
}

/// The groups of a match in lines of `buf` from line `firstlnum`.
#[derive(Clone)]
pub(super) struct SubmatchLines {
    buf: crate::globals::BufPtr,
    firstlnum: Linenr,
    maxline: Linenr,
    startpos: [Option<Lpos>; NSUBEXP],
    endpos: [Option<Lpos>; NSUBEXP],
}

impl SubmatchLines {
    /// Line `lnum` of the match, counted as for [`Regexec::getline`].
    fn getline(&self, lnum: Linenr) -> Option<RegLine<'static>> {
        let buf_lnum = self.firstlnum + lnum;
        if buf_lnum < 1 {
            return None;
        }
        if lnum > self.maxline {
            return Some(RegLine::default());
        }
        Some(RegLine::Buf(crate::memline::ml_get_buf(
            &self.buf, buf_lnum,
        )))
    }
}

/// The match that submatch() reads while a substitution evaluates a `\=` expression.
#[derive(Clone)]
pub(super) enum Regsubmatch {
    Text(SubmatchText),
    Lines(SubmatchLines),
}

impl Regsubmatch {
    /// The groups of the match `rex` has found.
    fn of(rex: &Regexec<'_>) -> Self {
        if rex.reg_multi() {
            return Self::Lines(SubmatchLines {
                buf: rex.reg_buf.clone(),
                firstlnum: rex.reg_firstlnum,
                maxline: rex.reg_maxline,
                startpos: rex.rmm_startpos.map(matched_pos),
                endpos: rex.rmm_endpos.map(matched_pos),
            });
        }
        let line = rex.line();
        let (lo, hi) = (0..NSUBEXP)
            .filter_map(|i| Some((rex.rm_startp[i]?, rex.rm_endp[i]?)))
            .fold((usize::MAX, 0), |(lo, hi), (s, e)| {
                (lo.min(s), hi.max(e.max(s)))
            });
        let (text_off, text) = if lo < hi {
            (lo, &line[lo.min(line.len())..hi.min(line.len())])
        } else {
            (0, &[][..])
        };
        Self::Text(SubmatchText {
            text: std::rc::Rc::from(text),
            text_off,
            startp: rex.rm_startp,
            endp: rex.rm_endp,
        })
    }
}

#[derive(Clone, Copy)]
struct Decomp {
    a: i32,
    b: i32,
    c: i32,
}

static DECOMP_TABLE: [Decomp; 0xfb4f - 0xfb20 + 1] = [
    Decomp {
        a: 0x5e2,
        b: 0,
        c: 0,
    }, // 0xfb20       alt ayin
    Decomp {
        a: 0x5d0,
        b: 0,
        c: 0,
    }, // 0xfb21       alt alef
    Decomp {
        a: 0x5d3,
        b: 0,
        c: 0,
    }, // 0xfb22       alt dalet
    Decomp {
        a: 0x5d4,
        b: 0,
        c: 0,
    }, // 0xfb23       alt he
    Decomp {
        a: 0x5db,
        b: 0,
        c: 0,
    }, // 0xfb24       alt kaf
    Decomp {
        a: 0x5dc,
        b: 0,
        c: 0,
    }, // 0xfb25       alt lamed
    Decomp {
        a: 0x5dd,
        b: 0,
        c: 0,
    }, // 0xfb26       alt mem-sofit
    Decomp {
        a: 0x5e8,
        b: 0,
        c: 0,
    }, // 0xfb27       alt resh
    Decomp {
        a: 0x5ea,
        b: 0,
        c: 0,
    }, // 0xfb28       alt tav
    Decomp {
        a: b'+' as i32,
        b: 0,
        c: 0,
    }, // 0xfb29       alt plus
    Decomp {
        a: 0x5e9,
        b: 0x5c1,
        c: 0,
    }, // 0xfb2a       shin+shin-dot
    Decomp {
        a: 0x5e9,
        b: 0x5c2,
        c: 0,
    }, // 0xfb2b       shin+sin-dot
    Decomp {
        a: 0x5e9,
        b: 0x5c1,
        c: 0x5bc,
    }, // 0xfb2c       shin+shin-dot+dagesh
    Decomp {
        a: 0x5e9,
        b: 0x5c2,
        c: 0x5bc,
    }, // 0xfb2d       shin+sin-dot+dagesh
    Decomp {
        a: 0x5d0,
        b: 0x5b7,
        c: 0,
    }, // 0xfb2e       alef+patah
    Decomp {
        a: 0x5d0,
        b: 0x5b8,
        c: 0,
    }, // 0xfb2f       alef+qamats
    Decomp {
        a: 0x5d0,
        b: 0x5b4,
        c: 0,
    }, // 0xfb30       alef+hiriq
    Decomp {
        a: 0x5d1,
        b: 0x5bc,
        c: 0,
    }, // 0xfb31       bet+dagesh
    Decomp {
        a: 0x5d2,
        b: 0x5bc,
        c: 0,
    }, // 0xfb32       gimel+dagesh
    Decomp {
        a: 0x5d3,
        b: 0x5bc,
        c: 0,
    }, // 0xfb33       dalet+dagesh
    Decomp {
        a: 0x5d4,
        b: 0x5bc,
        c: 0,
    }, // 0xfb34       he+dagesh
    Decomp {
        a: 0x5d5,
        b: 0x5bc,
        c: 0,
    }, // 0xfb35       vav+dagesh
    Decomp {
        a: 0x5d6,
        b: 0x5bc,
        c: 0,
    }, // 0xfb36       zayin+dagesh
    Decomp {
        a: 0xfb37,
        b: 0,
        c: 0,
    }, // 0xfb37 -- UNUSED
    Decomp {
        a: 0x5d8,
        b: 0x5bc,
        c: 0,
    }, // 0xfb38       tet+dagesh
    Decomp {
        a: 0x5d9,
        b: 0x5bc,
        c: 0,
    }, // 0xfb39       yud+dagesh
    Decomp {
        a: 0x5da,
        b: 0x5bc,
        c: 0,
    }, // 0xfb3a       kaf sofit+dagesh
    Decomp {
        a: 0x5db,
        b: 0x5bc,
        c: 0,
    }, // 0xfb3b       kaf+dagesh
    Decomp {
        a: 0x5dc,
        b: 0x5bc,
        c: 0,
    }, // 0xfb3c       lamed+dagesh
    Decomp {
        a: 0xfb3d,
        b: 0,
        c: 0,
    }, // 0xfb3d -- UNUSED
    Decomp {
        a: 0x5de,
        b: 0x5bc,
        c: 0,
    }, // 0xfb3e       mem+dagesh
    Decomp {
        a: 0xfb3f,
        b: 0,
        c: 0,
    }, // 0xfb3f -- UNUSED
    Decomp {
        a: 0x5e0,
        b: 0x5bc,
        c: 0,
    }, // 0xfb40       nun+dagesh
    Decomp {
        a: 0x5e1,
        b: 0x5bc,
        c: 0,
    }, // 0xfb41       samech+dagesh
    Decomp {
        a: 0xfb42,
        b: 0,
        c: 0,
    }, // 0xfb42 -- UNUSED
    Decomp {
        a: 0x5e3,
        b: 0x5bc,
        c: 0,
    }, // 0xfb43       pe sofit+dagesh
    Decomp {
        a: 0x5e4,
        b: 0x5bc,
        c: 0,
    }, // 0xfb44       pe+dagesh
    Decomp {
        a: 0xfb45,
        b: 0,
        c: 0,
    }, // 0xfb45 -- UNUSED
    Decomp {
        a: 0x5e6,
        b: 0x5bc,
        c: 0,
    }, // 0xfb46       tsadi+dagesh
    Decomp {
        a: 0x5e7,
        b: 0x5bc,
        c: 0,
    }, // 0xfb47       qof+dagesh
    Decomp {
        a: 0x5e8,
        b: 0x5bc,
        c: 0,
    }, // 0xfb48       resh+dagesh
    Decomp {
        a: 0x5e9,
        b: 0x5bc,
        c: 0,
    }, // 0xfb49       shin+dagesh
    Decomp {
        a: 0x5ea,
        b: 0x5bc,
        c: 0,
    }, // 0xfb4a       tav+dagesh
    Decomp {
        a: 0x5d5,
        b: 0x5b9,
        c: 0,
    }, // 0xfb4b       vav+holam
    Decomp {
        a: 0x5d1,
        b: 0x5bf,
        c: 0,
    }, // 0xfb4c       bet+rafe
    Decomp {
        a: 0x5db,
        b: 0x5bf,
        c: 0,
    }, // 0xfb4d       kaf+rafe
    Decomp {
        a: 0x5e4,
        b: 0x5bf,
        c: 0,
    }, // 0xfb4e       pe+rafe
    Decomp {
        a: 0x5d0,
        b: 0x5dc,
        c: 0,
    }, // 0xfb4f       alef-lamed
];

pub(super) struct RegexpState {
    pub(super) regparse_off: std::cell::Cell<usize>,
    pub(super) regnpar: std::cell::Cell<i32>,
    pub(super) wants_nfa: std::cell::Cell<bool>,
    pub(super) regnzpar: std::cell::Cell<i32>,
    pub(super) re_has_z: std::cell::Cell<i32>,
    pub(super) regflags: std::cell::Cell<u32>,
    pub(super) had_eol: std::cell::Cell<bool>,
    pub(super) reg_magic: std::cell::Cell<RegMagic>,
    pub(super) reg_string: std::cell::Cell<i32>,
    pub(super) reg_strict: std::cell::Cell<i32>,
    pub(super) curchr: std::cell::Cell<i32>,
    pub(super) prevchr: std::cell::Cell<i32>,
    pub(super) prevprevchr: std::cell::Cell<i32>,
    pub(super) nextchr: std::cell::Cell<i32>,
    pub(super) prevchr_len: std::cell::Cell<i32>,
    pub(super) at_start: std::cell::Cell<i32>,
    pub(super) prev_at_start: std::cell::Cell<i32>,
    pub(super) peekchr_after_slash: std::cell::Cell<i32>,
    pub(super) reg_cpo_lit: std::cell::Cell<i32>,
    /// The match a substitution evaluating a `\=` expression reads with submatch().
    pub(super) rsm: crate::globals::CloneCell<Option<Regsubmatch>>,
    pub(super) regsub_nesting: std::cell::Cell<usize>,
    pub(super) had_endbrace: [std::cell::Cell<u8>; NSUBEXP],
    pub(super) num_complex_braces: std::cell::Cell<i32>,
    pub(super) regsize: std::cell::Cell<i64>,
    pub(super) reg_toolong: std::cell::Cell<i32>,
    pub(super) bt_reg_parse_depth: std::cell::Cell<i32>,
    pub(super) one_exactly: std::cell::Cell<bool>,
    pub(super) nfa_re_flags: std::cell::Cell<i32>,
    pub(super) nfa_has_zend: std::cell::Cell<bool>,
    pub(super) nfa_has_backref: std::cell::Cell<bool>,
    pub(super) nfa_reg_parse_depth: std::cell::Cell<i32>,
    pub(super) nstate: std::cell::Cell<i32>,
    pub(super) istate: std::cell::Cell<i32>,
    pub(super) regexp_engine: std::cell::Cell<i32>,
    pub(super) reg_do_extmatch: std::cell::Cell<i32>,
    pub(super) re_extmatch_in: crate::globals::CloneCell<Option<std::rc::Rc<RegExtmatch>>>,
    pub(super) re_extmatch_out: crate::globals::CloneCell<Option<std::rc::Rc<RegExtmatch>>>,
}

impl RegexpState {
    const fn new() -> Self {
        Self {
            regparse_off: std::cell::Cell::new(0),
            regnpar: std::cell::Cell::new(0),
            wants_nfa: std::cell::Cell::new(false),
            regnzpar: std::cell::Cell::new(0),
            re_has_z: std::cell::Cell::new(0),
            regflags: std::cell::Cell::new(0),
            had_eol: std::cell::Cell::new(false),
            reg_magic: std::cell::Cell::new(0),
            reg_string: std::cell::Cell::new(0),
            reg_strict: std::cell::Cell::new(0),
            curchr: std::cell::Cell::new(0),
            prevchr: std::cell::Cell::new(0),
            prevprevchr: std::cell::Cell::new(0),
            nextchr: std::cell::Cell::new(0),
            prevchr_len: std::cell::Cell::new(0),
            at_start: std::cell::Cell::new(0),
            prev_at_start: std::cell::Cell::new(0),
            peekchr_after_slash: std::cell::Cell::new(0),
            reg_cpo_lit: std::cell::Cell::new(0),
            rsm: crate::globals::CloneCell::new(None),
            regsub_nesting: std::cell::Cell::new(0),
            had_endbrace: [const { std::cell::Cell::new(0) }; NSUBEXP],
            num_complex_braces: std::cell::Cell::new(0),
            regsize: std::cell::Cell::new(0),
            reg_toolong: std::cell::Cell::new(0),
            bt_reg_parse_depth: std::cell::Cell::new(0),
            one_exactly: std::cell::Cell::new(false),
            nfa_re_flags: std::cell::Cell::new(0),
            nfa_has_zend: std::cell::Cell::new(false),
            nfa_has_backref: std::cell::Cell::new(false),
            nfa_reg_parse_depth: std::cell::Cell::new(0),
            nstate: std::cell::Cell::new(0),
            istate: std::cell::Cell::new(0),
            regexp_engine: std::cell::Cell::new(0),
            reg_do_extmatch: std::cell::Cell::new(0),
            re_extmatch_in: crate::globals::CloneCell::new(None),
            re_extmatch_out: crate::globals::CloneCell::new(None),
        }
    }
}

#[inline]
pub(crate) fn set_reg_do_extmatch(v: i32) {
    RE_STATE.with(|s| s.reg_do_extmatch.set(v));
}

#[inline]
pub(crate) fn set_re_extmatch_in(v: Option<std::rc::Rc<RegExtmatch>>) {
    RE_STATE.with(|s| s.re_extmatch_in.set(v));
}

#[inline]
pub(crate) fn take_re_extmatch_out() -> Option<std::rc::Rc<RegExtmatch>> {
    RE_STATE.with(|s| s.re_extmatch_out.take())
}

#[inline]
pub(crate) fn clear_re_extmatch_out() {
    RE_STATE.with(|s| s.re_extmatch_out.set(None));
}

struct CachedRegprog {
    expr: Vec<u8>,
    re_flags: i32,
    engine: i32,
    cpo_lit: bool,
    chartab: [u64; 4],
    had_eol: bool,
    prog: Regprog,
}

thread_local! {
    pub(super) static RE_STATE: RegexpState = const { RegexpState::new() };
    static LAST_REGPROG: std::cell::RefCell<Option<CachedRegprog>> = const {
        std::cell::RefCell::new(None)
    };
    pub(super) static REGPARSE_BASE: std::cell::RefCell<Vec<u8>> = const {
        std::cell::RefCell::new(Vec::new())
    };
    pub(super) static REG_PREV_SUB: std::cell::RefCell<Option<std::ffi::CString>> = const {
        std::cell::RefCell::new(None)
    };
    pub(super) static EVAL_RESULT: [std::cell::RefCell<Option<std::ffi::CString>>; MAX_REGSUB_NESTING] = const {
        [const { std::cell::RefCell::new(None) }; MAX_REGSUB_NESTING]
    };
    pub(super) static POST_VEC: std::cell::RefCell<Vec<i32>> = const {
        std::cell::RefCell::new(Vec::new())
    };
}

#[inline]
pub(super) fn set_regparse_base(s: &[u8]) {
    REGPARSE_BASE.with(|cell| {
        let mut b = cell.borrow_mut();
        b.clear();
        b.extend_from_slice(crate::memory::bytes_until_nul(s));
    });
}

#[derive(Clone, Copy)]
pub(super) struct ReSt;

#[inline(always)]
pub(super) const fn re_st() -> ReSt {
    ReSt
}

impl ReSt {
    #[inline(always)]
    pub fn with_regparse_bytes<R>(self, f: impl FnOnce(&[u8]) -> R) -> R {
        RE_STATE.with(|s| s.with_regparse_bytes(f))
    }
    #[inline(always)]
    pub fn regparse_byte(self) -> u8 {
        RE_STATE.with(RegexpState::regparse_byte)
    }
    #[inline(always)]
    pub fn regparse_byte_at(self, idx: usize) -> u8 {
        RE_STATE.with(|s| s.regparse_byte_at(idx))
    }
    #[inline(always)]
    pub fn regparse_mb_adv(self) {
        RE_STATE.with(RegexpState::regparse_mb_adv);
    }
    #[inline(always)]
    pub fn regparse_prev_char(self) {
        RE_STATE.with(RegexpState::regparse_prev_char);
    }
    #[inline(always)]
    pub fn regparse_composing_len(self, char_len: usize) -> usize {
        RE_STATE.with(|s| s.regparse_composing_len(char_len))
    }
    #[inline(always)]
    pub fn regparse_get_char_class(self) -> i32 {
        RE_STATE.with(RegexpState::regparse_get_char_class)
    }
    #[inline(always)]
    pub fn regparse_get_equi_class(self) -> i32 {
        RE_STATE.with(RegexpState::regparse_get_equi_class)
    }
    #[inline(always)]
    pub fn regparse_get_coll_element(self) -> i32 {
        RE_STATE.with(RegexpState::regparse_get_coll_element)
    }
}

#[inline(always)]
pub(super) fn reg_prev_sub_ref() -> Option<std::ffi::CString> {
    REG_PREV_SUB.with(|c| c.borrow().clone())
}

use crate::eval::partial_name;
use crate::eval::typval::tv_get_string_buf_chk;
use crate::eval::userfunc::call_func;
use crate::strings::{vim_strchr, vim_strsave_escaped};

#[inline(always)]
pub(super) fn tv_list_ref(l: Option<&TvList>) {
    if let Some(l) = l {
        l.lv_refcount.set(l.lv_refcount.get() + 1);
    }
}

#[inline(always)]
pub(super) const fn magic_fn(x: i32) -> i32 {
    x - 256
}

#[inline(always)]
pub(super) const fn un_magic(x: i32) -> i32 {
    x + 256
}

#[inline(always)]
pub(super) const fn is_magic_fn(x: i32) -> bool {
    x < 0
}

#[inline(always)]
pub(super) const fn no_magic(x: i32) -> i32 {
    if is_magic_fn(x) { un_magic(x) } else { x }
}

#[inline(always)]
pub(super) const fn toggle_magic(x: i32) -> i32 {
    if is_magic_fn(x) {
        un_magic(x)
    } else {
        magic_fn(x)
    }
}

/// Return NOT_MULTI if c is not a "multi" operator.
/// Return MULTI_ONE if c is a single "multi" operator.
/// Return MULTI_MULT if c is a multi "multi" operator.
#[inline(always)]
pub(super) fn re_multi_type(c: i32) -> i32 {
    if c == magic_fn(b'@' as i32) || c == magic_fn(b'=' as i32) || c == magic_fn(b'?' as i32) {
        return MULTI_ONE;
    }
    if c == magic_fn(b'*' as i32) || c == magic_fn(b'+' as i32) || c == magic_fn(b'{' as i32) {
        return MULTI_MULT;
    }
    NOT_MULTI
}

/// Translate `\x` to its control character, except `\n`, which is Magic.
#[inline(always)]
pub(super) fn backslash_trans(c: i32) -> i32 {
    match c as u8 {
        b'r' => CAR,
        b't' => TAB,
        b'e' => ESC,
        b'b' => BS,
        _ => c,
    }
}

/// Check for a character class name `[:name:]`. `rem` starts at the `[`.
pub(super) fn get_char_class(rem: &mut &[u8]) -> i32 {
    const CHAR_CLASS_TAB: &[(i32, &[u8])] = &[
        (CLASS_ALNUM, b"alnum:]"),
        (CLASS_ALPHA, b"alpha:]"),
        (CLASS_BACKSPACE, b"backspace:]"),
        (CLASS_BLANK, b"blank:]"),
        (CLASS_CNTRL, b"cntrl:]"),
        (CLASS_DIGIT, b"digit:]"),
        (CLASS_ESCAPE, b"escape:]"),
        (CLASS_FNAME, b"fname:]"),
        (CLASS_GRAPH, b"graph:]"),
        (CLASS_IDENT, b"ident:]"),
        (CLASS_KEYWORD, b"keyword:]"),
        (CLASS_LOWER, b"lower:]"),
        (CLASS_PRINT, b"print:]"),
        (CLASS_PUNCT, b"punct:]"),
        (CLASS_RETURN, b"return:]"),
        (CLASS_SPACE, b"space:]"),
        (CLASS_TAB, b"tab:]"),
        (CLASS_UPPER, b"upper:]"),
        (CLASS_XDIGIT, b"xdigit:]"),
    ];

    let bytes = *rem;
    if bytes.get(1) == Some(&b':')
        && bytes.get(2).is_some_and(u8::is_ascii_lowercase)
        && bytes.get(3).is_some_and(u8::is_ascii_lowercase)
        && bytes.get(4).is_some_and(u8::is_ascii_lowercase)
    {
        let target = &bytes[2..];
        for &(key, pat) in CHAR_CLASS_TAB {
            if let Some(rest) = target.strip_prefix(pat) {
                *rem = rest;
                return key;
            }
        }
    }
    CLASS_NONE
}

pub(super) const RI_CLASS_TAB: [i16; 256] = {
    let mut tab = [0i16; 256];
    let mut i = 0usize;
    while i < 256 {
        let val = if i >= b'0' as usize && i <= b'7' as usize {
            RI_DIGIT + RI_HEX + RI_OCTAL + RI_WORD
        } else if i >= b'8' as usize && i <= b'9' as usize {
            RI_DIGIT + RI_HEX + RI_WORD
        } else if i >= b'a' as usize && i <= b'f' as usize {
            RI_HEX + RI_WORD + RI_HEAD + RI_ALPHA + RI_LOWER
        } else if i >= b'g' as usize && i <= b'z' as usize {
            RI_WORD + RI_HEAD + RI_ALPHA + RI_LOWER
        } else if i >= b'A' as usize && i <= b'F' as usize {
            RI_HEX + RI_WORD + RI_HEAD + RI_ALPHA + RI_UPPER
        } else if i >= b'G' as usize && i <= b'Z' as usize {
            RI_WORD + RI_HEAD + RI_ALPHA + RI_UPPER
        } else if i == b'_' as usize {
            RI_WORD + RI_HEAD
        } else {
            0
        };
        tab[i] = val;
        i += 1;
    }
    tab[b' ' as usize] |= RI_WHITE;
    tab[b'\t' as usize] |= RI_WHITE;
    tab
};

#[inline(always)]
pub(super) fn ri_digit(c: i32) -> bool {
    (0..0x100).contains(&c) && (RI_CLASS_TAB[c as usize] & RI_DIGIT) != 0
}
#[inline(always)]
pub(super) fn ri_hex(c: i32) -> bool {
    (0..0x100).contains(&c) && (RI_CLASS_TAB[c as usize] & RI_HEX) != 0
}
#[inline(always)]
pub(super) fn ri_octal(c: i32) -> bool {
    (0..0x100).contains(&c) && (RI_CLASS_TAB[c as usize] & RI_OCTAL) != 0
}
#[inline(always)]
pub(super) fn ri_word(c: i32) -> bool {
    (0..0x100).contains(&c) && (RI_CLASS_TAB[c as usize] & RI_WORD) != 0
}
#[inline(always)]
pub(super) fn ri_head(c: i32) -> bool {
    (0..0x100).contains(&c) && (RI_CLASS_TAB[c as usize] & RI_HEAD) != 0
}
#[inline(always)]
pub(super) fn ri_alpha(c: i32) -> bool {
    (0..0x100).contains(&c) && (RI_CLASS_TAB[c as usize] & RI_ALPHA) != 0
}
#[inline(always)]
pub(super) fn ri_lower(c: i32) -> bool {
    (0..0x100).contains(&c) && (RI_CLASS_TAB[c as usize] & RI_LOWER) != 0
}
#[inline(always)]
pub(super) fn ri_upper(c: i32) -> bool {
    (0..0x100).contains(&c) && (RI_CLASS_TAB[c as usize] & RI_UPPER) != 0
}

/// Return true if compiled regular expression `prog` can match a line break.
pub fn re_multiline(prog: &Regprog) -> i32 {
    (prog.regflags() & RF_HASNL) as i32
}

/// Check for an equivalence class name `[=a=]`. `rem` starts at the `[`.
pub(super) fn get_equi_class(rem: &mut &[u8]) -> i32 {
    let bytes = *rem;
    if bytes.len() >= 3 && bytes[1] == b'=' {
        let l = crate::mbyte::utfc_slice2len(&bytes[2..]);
        if bytes.get(l + 2..l + 4) == Some(b"=]") {
            let c = crate::mbyte::utf_slice2char(&bytes[2..]);
            *rem = &bytes[l + 4..];
            return c;
        }
    }
    0
}

/// Check for a collating element `[.a.]`. `rem` starts at the `[`.
pub(super) fn get_coll_element(rem: &mut &[u8]) -> i32 {
    let bytes = *rem;
    if bytes.len() >= 3 && bytes[1] == b'.' {
        let l = crate::mbyte::utfc_slice2len(&bytes[2..]);
        if bytes.get(l + 2..l + 4) == Some(b".]") {
            let c = crate::mbyte::utf_slice2char(&bytes[2..]);
            *rem = &bytes[l + 4..];
            return c;
        }
    }
    0
}

pub(super) fn get_cpo_flags() {
    RE_STATE.with(|s| {
        s.reg_cpo_lit
            .set((crate::strings::vim_haschr(&opt_vars().p_cpo.as_cstr(), K_CPO_LITERAL)) as i32)
    });
}

/// Skip over a `[]` range in a byte slice.
pub(super) fn skip_anyof_slice(bytes: &[u8]) -> &[u8] {
    let mut rem = crate::memory::bytes_until_nul(bytes);
    if let Some(rest) = rem.strip_prefix(b"^") {
        rem = rest;
    }
    if matches!(rem.first(), Some(b']' | b'-')) {
        rem = &rem[1..];
    }
    while let Some(&b0) = rem.first() {
        if b0 == b']' {
            break;
        }
        let l = crate::mbyte::utfc_slice2len(rem);
        if l > 1 {
            rem = &rem[l..];
        } else if b0 == b'-' {
            rem = &rem[1..];
            if let Some(&b1) = rem.first()
                && b1 != b']'
            {
                let step = crate::mbyte::utfc_slice2len(rem);
                rem = &rem[step..];
            }
        } else if b0 == b'\\'
            && let Some(&b1) = rem.get(1)
            && (REGEXP_INRANGE.to_bytes().contains(&b1)
                || (RE_STATE.with(|s| s.reg_cpo_lit.get()) == 0
                    && REGEXP_ABBR.to_bytes().contains(&b1)))
        {
            rem = &rem[2..];
        } else if b0 == b'[' {
            if get_char_class(&mut rem) == CLASS_NONE
                && get_equi_class(&mut rem) == 0
                && get_coll_element(&mut rem) == 0
                && !rem.is_empty()
            {
                rem = &rem[1..];
            }
        } else {
            rem = &rem[1..];
        }
    }
    rem
}

/// Skip past regular expression in a byte slice.
pub fn skip_regexp_slice(start: &[u8], dirc: i32, magic: i32) -> &[u8] {
    let mut rem = crate::memory::bytes_until_nul(start);
    let mut mymagic = if magic != 0 { MAGIC_ON } else { MAGIC_OFF };

    get_cpo_flags();

    while let Some(&b0) = rem.first() {
        if i32::from(b0) == dirc {
            break;
        }
        if (b0 == b'[' && mymagic >= MAGIC_ON) || (rem.starts_with(b"\\[") && mymagic <= MAGIC_OFF)
        {
            rem = skip_anyof_slice(&rem[1..]);
            if rem.is_empty() {
                break;
            }
        } else if b0 == b'\\' && rem.len() > 1 {
            rem = &rem[1..];
            let next_b = rem.first().copied().unwrap_or(0);
            if next_b == b'v' {
                mymagic = MAGIC_ALL;
            } else if next_b == b'V' {
                mymagic = MAGIC_NONE;
            }
        }
        let step = crate::mbyte::utfc_slice2len(rem).max(1);
        rem = &rem[step.min(rem.len())..];
    }
    rem
}

/// Skip past regular expression.
pub fn skip_regexp(start: &[u8], delim: i32, magic: i32) -> &[u8] {
    skip_regexp_slice(start, delim, magic)
}

/// Call `skip_regexp()` and when the delimiter does not match give an error and return `None`.
pub fn skip_regexp_err(start: &[u8], delim: i32, magic: i32) -> Option<&[u8]> {
    let bytes = crate::memory::bytes_until_nul(start);
    let p = skip_regexp_slice(bytes, delim, magic);
    if p.first().copied().unwrap_or(0) as i32 != delim {
        semsg!(
            tr(E_MISSING_DELIMITER_AFTER_SEARCH_PATTERN_STR),
            crate::memory::bytes_to_cstring(bytes).as_c_str(),
        );
        return None;
    }
    Some(p)
}

/// Skip past regular expression without mutating `start`, returning the byte offset and final magic state.
pub fn skip_regexp_magic(
    start: &[u8],
    dirc: i32,
    magic: i32,
    mut magic_val: Option<&mut RegMagic>,
) -> usize {
    let bytes = crate::memory::bytes_until_nul(start);
    let mut rem = bytes;
    let mut mymagic = if magic != 0 { MAGIC_ON } else { MAGIC_OFF };

    get_cpo_flags();

    while let Some(&b0) = rem.first() {
        if i32::from(b0) == dirc {
            break;
        }
        if (b0 == b'[' && mymagic >= MAGIC_ON) || (rem.starts_with(b"\\[") && mymagic <= MAGIC_OFF)
        {
            rem = skip_anyof_slice(&rem[1..]);
            if rem.is_empty() {
                break;
            }
        } else if b0 == b'\\' && rem.len() > 1 {
            rem = &rem[1..];
            let next_b = rem.first().copied().unwrap_or(0);
            if next_b == b'v' {
                mymagic = MAGIC_ALL;
            } else if next_b == b'V' {
                mymagic = MAGIC_NONE;
            }
        }
        let step = crate::mbyte::utfc_slice2len(rem).max(1);
        rem = &rem[step.min(rem.len())..];
    }
    if let Some(ref mut p) = magic_val {
        **p = mymagic;
    }
    bytes.len() - rem.len()
}

/// `skip_regexp()` with in-place backslash-question removal, returning the byte offset.
pub fn skip_regexp_ex(
    start: &mut [u8],
    dirc: i32,
    magic: i32,
    mut copy_buf: Option<&mut Vec<u8>>,
    mut dropped: Option<&mut i32>,
    mut magic_val: Option<&mut RegMagic>,
) -> usize {
    let mut mymagic = if magic != 0 { MAGIC_ON } else { MAGIC_OFF };
    let mut startplen = crate::memory::bytes_until_nul(start).len();
    let mut off: usize = 0;
    let mut use_copy = false;

    get_cpo_flags();

    loop {
        let active: &mut [u8] = if use_copy {
            copy_buf.as_deref_mut().unwrap().as_mut_slice()
        } else {
            start
        };
        let p_bytes = &active[off..startplen];
        let Some(&b0) = p_bytes.first() else {
            break;
        };
        if b0 as i32 == dirc {
            break;
        }
        if (b0 == b'[' && mymagic >= MAGIC_ON)
            || (p_bytes.starts_with(b"\\[") && mymagic <= MAGIC_OFF)
        {
            let rem = skip_anyof_slice(&p_bytes[1..]);
            off += 1 + (p_bytes.len() - 1 - rem.len());
            if rem.is_empty() {
                break;
            }
        } else if b0 == b'\\' && p_bytes.len() > 1 {
            if dirc == b'?' as i32 && p_bytes[1] == b'?' {
                if let Some(buf) = copy_buf.as_deref_mut()
                    && !use_copy
                {
                    buf.clear();
                    buf.extend_from_slice(&start[..=startplen]);
                    use_copy = true;
                }
                if let Some(ref mut p) = dropped {
                    **p += 1;
                }
                let target: &mut [u8] = if use_copy {
                    copy_buf.as_deref_mut().unwrap().as_mut_slice()
                } else {
                    start
                };
                target.copy_within(off + 1..=startplen, off);
                startplen -= 1;
            } else {
                off += 1;
            }
            let target: &[u8] = if use_copy {
                copy_buf.as_deref().unwrap().as_slice()
            } else {
                start
            };
            let next_b = target.get(off).copied().unwrap_or(0);
            if next_b == b'v' {
                mymagic = MAGIC_ALL;
            } else if next_b == b'V' {
                mymagic = MAGIC_NONE;
            }
        }
        let target: &[u8] = if use_copy {
            copy_buf.as_deref().unwrap().as_slice()
        } else {
            start
        };
        let rem = &target[off..startplen];
        if rem.is_empty() {
            break;
        }
        let step = crate::mbyte::utfc_slice2len(rem).max(1).min(rem.len());
        off += step;
    }
    if let Some(ref mut p) = magic_val {
        **p = mymagic;
    }
    off
}

pub(super) fn initchr(str_: &[u8]) {
    set_regparse_base(str_);
    RE_STATE.with(|s| s.regparse_off.set(0));
    RE_STATE.with(|s| s.prevchr_len.set(0));
    RE_STATE.with(|s| s.nextchr.set(-1));
    RE_STATE.with(|s| s.prevchr.set(-1));
    RE_STATE.with(|s| s.prevprevchr.set(-1));
    RE_STATE.with(|s| s.curchr.set(-1));
    RE_STATE.with(|s| s.at_start.set(1));
    RE_STATE.with(|s| s.prev_at_start.set(0));
}

pub(super) fn save_parse_state(ps: &mut ParseState) {
    RE_STATE.with(|re_s| {
        ps.regparse_off = re_s.regparse_off.get();
        ps.prevchr_len = re_s.prevchr_len.get();
        ps.curchr = re_s.curchr.get();
        ps.prevchr = re_s.prevchr.get();
        ps.prevprevchr = re_s.prevprevchr.get();
        ps.nextchr = re_s.nextchr.get();
        ps.at_start = re_s.at_start.get();
        ps.prev_at_start = re_s.prev_at_start.get();
        ps.regnpar = re_s.regnpar.get();
    })
}

pub(super) fn restore_parse_state(ps: &mut ParseState) {
    RE_STATE.with(|re_s| {
        re_s.regparse_off.set(ps.regparse_off);
        re_s.prevchr_len.set(ps.prevchr_len);
        re_s.curchr.set(ps.curchr);
        re_s.prevchr.set(ps.prevchr);
        re_s.prevprevchr.set(ps.prevprevchr);
        re_s.nextchr.set(ps.nextchr);
        re_s.at_start.set(ps.at_start);
        re_s.prev_at_start.set(ps.prev_at_start);
        re_s.regnpar.set(ps.regnpar);
    })
}

pub(super) fn peekchr() -> i32 {
    RE_STATE.with(|re_s| {
        if re_s.curchr.get() != -1 {
            return re_s.curchr.get();
        }

        re_s.curchr.set(re_st().regparse_byte() as i32);
        match re_s.curchr.get() as u8 {
            b'.' | b'[' | b'~' => {
                if re_s.reg_magic.get() >= MAGIC_ON {
                    re_s.curchr.set(magic_fn(re_s.curchr.get()));
                }
            }
            b'(' | b')' | b'{' | b'%' | b'+' | b'=' | b'?' | b'@' | b'!' | b'&' | b'|' | b'<'
            | b'>' | b'#' | b'"' | b'\'' | b',' | b'-' | b':' | b';' | b'`' | b'/' => {
                if re_s.reg_magic.get() == MAGIC_ALL {
                    re_s.curchr.set(magic_fn(re_s.curchr.get()));
                }
            }
            b'*' => {
                if re_s.reg_magic.get() >= MAGIC_ON
                    && re_s.at_start.get() == 0
                    && !(re_s.prev_at_start.get() != 0
                        && re_s.prevchr.get() == magic_fn(b'^' as i32))
                    && (re_s.peekchr_after_slash.get() != 0
                        || (re_s.prevchr.get() != magic_fn(b'(' as i32)
                            && re_s.prevchr.get() != magic_fn(b'&' as i32)
                            && re_s.prevchr.get() != magic_fn(b'|' as i32)))
                {
                    re_s.curchr.set(magic_fn(b'*' as i32));
                }
            }
            b'^' => {
                if re_s.reg_magic.get() >= MAGIC_OFF
                    && (re_s.at_start.get() != 0
                        || re_s.reg_magic.get() == MAGIC_ALL
                        || re_s.prevchr.get() == magic_fn(b'(' as i32)
                        || re_s.prevchr.get() == magic_fn(b'|' as i32)
                        || re_s.prevchr.get() == magic_fn(b'&' as i32)
                        || re_s.prevchr.get() == magic_fn(b'n' as i32)
                        || (no_magic(re_s.prevchr.get()) == b'(' as i32
                            && re_s.prevprevchr.get() == magic_fn(b'%' as i32)))
                {
                    re_s.curchr.set(magic_fn(b'^' as i32));
                    re_s.at_start.set(1);
                    re_s.prev_at_start.set(0);
                }
            }
            b'$' => {
                if re_s.reg_magic.get() >= MAGIC_OFF {
                    let is_eol = re_st().with_regparse_bytes(|rb| {
                        let mut p_bytes = &rb[1..];
                        let mut is_magic_all = re_s.reg_magic.get() == MAGIC_ALL;

                        while let Some(&next_b) = p_bytes.get(1) {
                            if p_bytes[0] != b'\\'
                                || !matches!(next_b, b'c' | b'C' | b'm' | b'M' | b'v' | b'V' | b'Z')
                            {
                                break;
                            }
                            if next_b == b'v' {
                                is_magic_all = true;
                            } else if matches!(next_b, b'm' | b'M' | b'V') {
                                is_magic_all = false;
                            }
                            p_bytes = &p_bytes[2..];
                        }
                        let p_b = p_bytes.first().copied().unwrap_or(0);
                        p_b == NUL
                            || (p_b == b'\\'
                                && p_bytes
                                    .get(1)
                                    .is_some_and(|&b| matches!(b, b'|' | b'&' | b')' | b'n')))
                            || (is_magic_all && matches!(p_b, b'|' | b'&' | b')'))
                            || re_s.reg_magic.get() == MAGIC_ALL
                    });
                    if is_eol {
                        re_s.curchr.set(magic_fn(b'$' as i32));
                    }
                }
            }
            b'\\' => {
                let c = i32::from(re_st().regparse_byte_at(1));
                if c == NUL as i32 {
                    re_s.curchr.set(b'\\' as i32);
                } else if c <= b'~' as i32 && META_FLAGS[c as usize] != 0 {
                    re_s.curchr.set(-1);
                    re_s.prev_at_start.set(re_s.at_start.get());
                    re_s.at_start.set(0);
                    re_s.regparse_off.set(re_s.regparse_off.get() + (1));
                    re_s.peekchr_after_slash
                        .set(re_s.peekchr_after_slash.get() + (1));
                    let _ = peekchr();
                    re_s.regparse_off.set(re_s.regparse_off.get() - (1));
                    re_s.peekchr_after_slash
                        .set(re_s.peekchr_after_slash.get() - (1));
                    RE_STATE.with(|s| s.curchr.set(toggle_magic(re_s.curchr.get())));
                } else if crate::strings::vim_haschr(REGEXP_ABBR, c) {
                    re_s.curchr.set(backslash_trans(c));
                } else if re_s.reg_magic.get() == MAGIC_NONE
                    && (c == b'$' as i32 || c == b'^' as i32)
                {
                    re_s.curchr.set(toggle_magic(c));
                } else {
                    re_s.curchr.set(if c >= 0x80 {
                        re_st().with_regparse_bytes(|rb| utf_ptr2char(&rb[1..]))
                    } else {
                        c
                    });
                }
            }
            _ => {
                if re_s.curchr.get() >= 0x80 {
                    RE_STATE.with(|s| {
                        s.curchr
                            .set(re_st().with_regparse_bytes(|rb| utf_ptr2char(rb)))
                    });
                }
            }
        }

        re_s.curchr.get()
    })
}

pub(super) fn skipchr() {
    RE_STATE.with(|re_s| {
        if re_st().regparse_byte() == b'\\' {
            re_s.prevchr_len.set(1);
        } else {
            re_s.prevchr_len.set(0);
        }
        let next_b = re_st().regparse_byte_at(re_s.prevchr_len.get() as usize);
        if next_b != NUL {
            if next_b < 0x80 {
                re_s.prevchr_len.set(re_s.prevchr_len.get() + (1));
            } else {
                let off = re_s.prevchr_len.get() as usize;
                re_s.prevchr_len.set(
                    re_s.prevchr_len.get()
                        + re_st().with_regparse_bytes(|rb| utf_ptr2len(&rb[off..])),
                );
            }
        }
        RE_STATE.with(|s| {
            s.regparse_off
                .set(s.regparse_off.get() + (re_s.prevchr_len.get() as usize))
        });
        re_s.prev_at_start.set(re_s.at_start.get());
        re_s.at_start.set(0);
        re_s.prevprevchr.set(re_s.prevchr.get());
        re_s.prevchr.set(re_s.curchr.get());
        re_s.curchr.set(re_s.nextchr.get());
        re_s.nextchr.set(-1);
    })
}

pub(super) fn skipchr_keepstart() {
    RE_STATE.with(|re_s| {
        let as_ = re_s.prev_at_start.get();
        let pr = re_s.prevchr.get();
        let prpr = re_s.prevprevchr.get();

        skipchr();
        re_s.at_start.set(as_);
        re_s.prevchr.set(pr);
        re_s.prevprevchr.set(prpr);
    })
}

pub(super) fn getchr() -> i32 {
    let chr = peekchr();
    skipchr();
    chr
}

pub(super) fn ungetchr() {
    RE_STATE.with(|re_s| {
        re_s.nextchr.set(re_s.curchr.get());
        re_s.curchr.set(re_s.prevchr.get());
        re_s.prevchr.set(re_s.prevprevchr.get());
        re_s.at_start.set(re_s.prev_at_start.get());
        re_s.prev_at_start.set(0);
        RE_STATE.with(|s| {
            s.regparse_off
                .set(s.regparse_off.get() - (re_s.prevchr_len.get() as usize))
        });
    })
}

pub(super) fn gethexchrs(maxinputlen: i32) -> i64 {
    let mut nr: i64 = 0;
    let mut i: i32 = 0;
    while i < maxinputlen {
        let c = re_st().regparse_byte() as i32;
        if !ascii_isxdigit(c) {
            break;
        }
        nr <<= 4;
        nr |= i64::from(hex2nr(c));
        RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() + (1)));
        i += 1;
    }
    if i == 0 { -1 } else { nr }
}

pub(super) fn getdecchrs() -> i64 {
    let mut nr: i64 = 0;
    let mut i: i32 = 0;
    loop {
        let c = re_st().regparse_byte();
        if !c.is_ascii_digit() {
            break;
        }
        if nr <= i64::from(i32::MAX / 10) {
            nr = nr * 10 + i64::from(c - b'0');
        } else {
            nr = i64::from(i32::MAX) + 1;
        }
        RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() + (1)));
        RE_STATE.with(|s| s.curchr.set(-1));
        i += 1;
    }
    if i == 0 { -1 } else { nr }
}

pub(super) fn getoctchrs() -> i64 {
    let mut nr: i64 = 0;
    let mut i: i32 = 0;
    while i < 3 && nr < 0o40 {
        let c = re_st().regparse_byte();
        if !(b'0'..=b'7').contains(&c) {
            break;
        }
        nr <<= 3;
        nr |= i64::from(hex2nr(c as i32));
        RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() + (1)));
        i += 1;
    }
    if i == 0 { -1 } else { nr }
}

pub(super) fn read_limits(minval: &mut i32, maxval: &mut i32) -> bool {
    let mut reverse = false;
    if re_st().regparse_byte() == b'-' {
        RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() + (1)));
        reverse = true;
    }
    let first_char_b = re_st().regparse_byte();
    {
        let adv = re_st().with_regparse_bytes(|start_bytes| {
            let mut rem = start_bytes;
            *minval = getdigits_int(&mut rem, false, 0);
            start_bytes.len() - rem.len()
        });
        RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() + (adv)));
    }
    if re_st().regparse_byte() == b',' {
        RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() + (1)));
        if ascii_isdigit(re_st().regparse_byte() as i32) {
            let adv = re_st().with_regparse_bytes(|start_bytes| {
                let mut rem = start_bytes;
                *maxval = getdigits_int(&mut rem, false, MAX_LIMIT);
                start_bytes.len() - rem.len()
            });
            RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() + (adv)));
        } else {
            *maxval = MAX_LIMIT;
        }
    } else if ascii_isdigit(first_char_b as i32) {
        *maxval = *minval;
    } else {
        *maxval = MAX_LIMIT;
    }
    if re_st().regparse_byte() == b'\\' {
        RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() + (1)));
    }
    if re_st().regparse_byte() != b'}' {
        semsg!(
            tr(c"E554: Syntax error in %s{...}"),
            if RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL {
                c""
            } else {
                c"\\"
            },
        );
        msg_state().rc_did_emsg.set(true);
        return false;
    }
    if (!reverse && *minval > *maxval) || (reverse && *minval < *maxval) {
        std::mem::swap(minval, maxval);
    }
    skipchr();
    true
}

impl Regexec<'_> {
    /// Whether the current position is in the Visual area of the current buffer.
    pub(super) fn match_visual(&self) -> bool {
        let wp = if self.reg_win.is_null() {
            curwin()
        } else {
            self.reg_win.clone()
        };
        if self.reg_buf != workspace_state().curbuf.get()
            || edit_state().visual_global.start.get().lnum == 0
            || !self.reg_multi()
        {
            return false;
        }

        let (top, bot, mode, curswant) = if edit_state().visual_global.active.get() {
            let (t, b) = if lt(edit_state().visual_global.start.get(), wp.w_cursor.get()) {
                (edit_state().visual_global.start.get(), wp.w_cursor.get())
            } else {
                (wp.w_cursor.get(), edit_state().visual_global.start.get())
            };
            (
                t,
                b,
                edit_state().visual_global.mode.get(),
                wp.w_curswant.get(),
            )
        } else {
            let vis = curbuf().b_visual.get();
            let (t, mut b) = if lt(vis.vi_start, vis.vi_end) {
                (vis.vi_start, vis.vi_end)
            } else {
                (vis.vi_end, vis.vi_start)
            };
            if b.lnum > curbuf().b_ml.ml_line_count.get() {
                b.lnum = curbuf().b_ml.ml_line_count.get();
            }
            (t, b, vis.vi_mode, vis.vi_curswant)
        };

        let lnum = self.lnum + self.reg_firstlnum;
        if lnum < top.lnum || lnum > bot.lnum {
            return false;
        }

        let col = self.input_col();
        if mode == b'v' as i32 {
            if (lnum == top.lnum && col < top.col)
                || (lnum == bot.lnum
                    && col >= bot.col + Colnr::from(opt_vars().p_sel.first_byte() != b'e'))
            {
                return false;
            }
        } else if mode == CTRL_V_CONST {
            let mut start: Colnr = 0;
            let mut end: Colnr = 0;
            let mut start2: Colnr = 0;
            let mut end2: Colnr = 0;
            let mut top_copy = top;
            let mut bot_copy = bot;
            getvvcol(
                &wp,
                &mut top_copy,
                Some(&mut start),
                None,
                Some(&mut end),
                0,
            );
            getvvcol(
                &wp,
                &mut bot_copy,
                Some(&mut start2),
                None,
                Some(&mut end2),
                0,
            );
            if start2 < start {
                start = start2;
            }
            if end2 > end {
                end = end2;
            }
            if top.col == MAXCOL || bot.col == MAXCOL || curswant == MAXCOL {
                end = MAXCOL;
            }

            let cols = crate::plines::win_linetabsize_bytes(&wp, lnum, self.line(), col);
            if cols < start || cols > end - Colnr::from(opt_vars().p_sel.first_byte() == b'e') {
                return false;
            }
        }
        true
    }

    /// Matches the text from line `start_lnum`, column `start_col` to line `end_lnum`, column
    /// `end_col` at the current position, for a backreference in buffer lines; `bytelen` is
    /// set to the length matched in the last line.
    pub(super) fn match_with_backref(
        &mut self,
        start_lnum: Linenr,
        start_col: Colnr,
        end_lnum: Linenr,
        end_col: Colnr,
        bytelen: &mut i32,
    ) -> RegAction {
        let mut clnum = start_lnum;
        let mut ccol = start_col;

        *bytelen = 0;
        loop {
            let line = self
                .getline(clnum)
                .expect("a backreference is in a line the match has read");
            let mut len = if clnum == end_lnum {
                end_col - ccol
            } else {
                self.getline_len(clnum) - ccol
            };

            let sub = line.bytes().get(ccol as usize..).unwrap_or(&[]);
            let input = self.rest();
            if (!self.reg_ic && self.cstrncmp(sub, input, &mut len) != 0)
                || (self.reg_ic && mb_strnicmp(sub, input, len as usize) != 0)
            {
                return RegAction::NoMatch;
            }
            *bytelen += len;
            if clnum == end_lnum {
                break;
            }
            if self.lnum >= self.reg_maxline {
                return RegAction::NoMatch;
            }

            self.nextline();
            *bytelen = 0;
            clnum += 1;
            ccol = 0;
            if input_state().got_int.get() {
                return RegAction::Fail;
            }
        }
        RegAction::Match
    }
}
pub(super) fn re_mult_next(what: &std::ffi::CStr) -> bool {
    if re_multi_type(peekchr()) == MULTI_MULT {
        semsg!(tr(c"E888: (NFA regexp) cannot repeat %s"), what);
        msg_state().rc_did_emsg.set(true);
        return false;
    }
    true
}

fn mb_decompose(c: i32, c1: &mut i32, c2: &mut i32, c3: &mut i32) {
    if (0xfb20..=0xfb4f).contains(&c) {
        let d = DECOMP_TABLE[(c - 0xfb20) as usize];
        *c1 = d.a;
        *c2 = d.b;
        *c3 = d.c;
    } else {
        *c1 = c;
        *c2 = 0;
        *c3 = 0;
    }
}

impl Regexec<'_> {
    /// Compares at most `*n` bytes of `s1` and `s2`, strings without NUL, ignoring case when
    /// `reg_ic` is set; on a match ignoring case `*n` is set to the length matched in `s2`.
    pub(super) fn cstrncmp(&self, s1_b: &[u8], s2_b: &[u8], n: &mut i32) -> i32 {
        let mut result = if !self.reg_ic {
            let max_n = *n as usize;
            if s1_b.len() < max_n && s1_b.len() == s2_b.len() {
                s1_b.cmp(s2_b) as i32
            } else if s1_b.len() < max_n {
                if s2_b.starts_with(s1_b) {
                    -1
                } else {
                    s1_b.cmp(&s2_b[..s1_b.len().min(s2_b.len())]) as i32
                }
            } else if s2_b.len() < max_n {
                if s1_b.starts_with(s2_b) {
                    1
                } else {
                    s1_b[..s2_b.len()].cmp(s2_b) as i32
                }
            } else {
                s1_b[..max_n].cmp(&s2_b[..max_n]) as i32
            }
        } else {
            let mut rem1 = s1_b;
            let mut n2: i32 = 0;
            let mut n1 = *n;
            while n1 > 0 && !rem1.is_empty() {
                let step = crate::mbyte::utf_slice2len(rem1);
                n1 -= step as i32;
                rem1 = &rem1[step..];
                n2 += 1;
            }
            let mut rem2 = s2_b;
            while n2 > 0 && !rem2.is_empty() {
                n2 -= 1;
                let step = crate::mbyte::utf_slice2len(rem2);
                rem2 = &rem2[step..];
            }
            n2 = (s2_b.len() - rem2.len()) as i32;
            let r = utf_strnicmp(s1_b, s2_b, *n as usize, n2 as usize);
            if r == 0 {
                *n = n2;
            }
            r
        };

        if result != 0 && self.reg_icombine {
            let mut rem1 = s1_b;
            let mut rem2 = s2_b;
            let mut c1: i32 = 0;
            let mut c2: i32 = 0;
            let mut c11: i32 = 0;
            let mut c12: i32 = 0;
            let mut junk: i32 = 0;
            let mut junk2: i32 = 0;

            // Both strings can end before "*n" bytes, and neither advances past its end.
            while ((s1_b.len() - rem1.len()) as i32) < *n && !(rem1.is_empty() && rem2.is_empty()) {
                c1 = mb_ptr2char_adv(&mut rem1);
                c2 = mb_ptr2char_adv(&mut rem2);

                if c1 != c2 && (!self.reg_ic || utf_fold(c1) != utf_fold(c2)) {
                    mb_decompose(c1, &mut c11, &mut junk, &mut junk2);
                    mb_decompose(c2, &mut c12, &mut junk, &mut junk2);
                    c1 = c11;
                    c2 = c12;
                    if c11 != c12 && (!self.reg_ic || utf_fold(c11) != utf_fold(c12)) {
                        break;
                    }
                }
            }
            result = c2 - c1;
            if result == 0 {
                *n = (s2_b.len() - rem2.len()) as i32;
            }
        }

        result
    }

    #[inline(always)]
    pub(super) fn cstrchr(&self, s_bytes: &[u8], c: i32) -> Option<usize> {
        cstrchr_ic(s_bytes, c, self.reg_ic)
    }
}

#[inline(always)]
pub(crate) fn cstrchr_ic(s_bytes: &[u8], c: i32, reg_ic: bool) -> Option<usize> {
    if !reg_ic {
        return crate::strings::vim_strchr_pos(s_bytes, c);
    }

    let (cc, lc) = if c > 0x80 {
        let f = utf_fold(c);
        (f, f)
    } else if (c as u8).is_ascii_uppercase() {
        let l = (c as u8).to_ascii_lowercase() as i32;
        (l, l)
    } else if (c as u8).is_ascii_lowercase() {
        let u = (c as u8).to_ascii_uppercase() as i32;
        (u, c)
    } else {
        return crate::strings::vim_strchr_pos(s_bytes, c);
    };

    let mut idx = 0usize;
    if c < 0x80 {
        let cu = c as u8;
        let ccu = cc as u8;
        while idx < s_bytes.len() {
            let b0 = s_bytes[idx];
            if b0 < 0x80 {
                if b0 == cu || b0 == ccu {
                    return Some(idx);
                }
                idx += 1;
            } else {
                let rem = &s_bytes[idx..];
                let uc = crate::mbyte::utf_slice2char(rem);
                if (uc < 0x80 || uc != i32::from(b0)) && utf_fold(uc) == lc {
                    return Some(idx);
                }
                idx += crate::mbyte::utfc_slice2len(rem);
            }
        }
        return None;
    }

    while idx < s_bytes.len() {
        let rem = &s_bytes[idx..];
        let b0 = s_bytes[idx] as i32;
        let uc = crate::mbyte::utf_slice2char(rem);
        if uc == c || uc == cc {
            return Some(idx);
        }
        if (uc < 0x80 || uc != b0) && utf_fold(uc) == lc {
            return Some(idx);
        }
        idx += crate::mbyte::utfc_slice2len(rem);
    }
    None
}

/// Replace tildes in the pattern by the old pattern.
/// Returns `Some(CString)` if a new string was allocated, or `None` if `source` was modified in place.
pub fn regtilde(source: &mut [u8], magic: i32, preview: bool) -> Option<std::ffi::CString> {
    let mut newsub_vec: Option<Vec<u8>> = None;
    let mut newsublen = crate::memory::bytes_until_nul(source).len();
    let tilde: &[u8] = if magic == 0 { b"\\~" } else { b"~" };
    let tildelen = tilde.len();
    let mut error = false;

    let mut idx = 0usize;
    while idx < newsublen {
        let cur_slice: &[u8] = match newsub_vec {
            Some(ref v) => &v[..newsublen],
            None => &source[..newsublen],
        };
        if cur_slice[idx..].starts_with(tilde) {
            let prefixlen = idx;
            newsublen -= tildelen;
            let postfixlen = newsublen - prefixlen;
            let prev_sub_ref = reg_prev_sub_ref();
            let reg_prev_sublen = prev_sub_ref.as_ref().map_or(0, |s| s.to_bytes().len());
            let tmpsublen = prefixlen + reg_prev_sublen + postfixlen;

            if tmpsublen > 0
                && let Some(prev_sub) = prev_sub_ref
            {
                if tmpsublen > MAXCOL as usize {
                    emsg(tr(E_RESULTING_TEXT_TOO_LONG));
                    error = true;
                    break;
                }

                let mut tmpsub_vec: Vec<u8> = Vec::with_capacity(tmpsublen + 1);
                tmpsub_vec.extend_from_slice(&cur_slice[..prefixlen]);
                tmpsub_vec.extend_from_slice(prev_sub.to_bytes());
                tmpsub_vec.extend_from_slice(&cur_slice[prefixlen + tildelen..]);
                tmpsub_vec.push(0);

                newsub_vec = Some(tmpsub_vec);
                newsublen = tmpsublen;
                idx = prefixlen + reg_prev_sublen;
            } else {
                let buf: &mut [u8] = match newsub_vec {
                    Some(ref mut v) => v.as_mut_slice(),
                    None => source,
                };
                buf.copy_within(
                    prefixlen + tildelen..=prefixlen + tildelen + postfixlen,
                    prefixlen,
                );
            }
            continue;
        }
        if cur_slice[idx] == b'\\' && idx + 1 < newsublen {
            idx += 1;
        }
        idx += crate::mbyte::utfc_slice2len(&cur_slice[idx..]);
    }

    if error {
        return None;
    }

    if !preview {
        let new_prev = if newsublen == 0 {
            None
        } else {
            let cur_slice: &[u8] = match newsub_vec {
                Some(ref v) => &v[..newsublen],
                None => &source[..newsublen],
            };
            cstring_from_bytes(cur_slice)
        };
        REG_PREV_SUB.with(|c| *c.borrow_mut() = new_prev);
    }

    newsub_vec.map(|v| crate::memory::cstring_until_nul(&v))
}

pub(crate) fn fill_submatch_list(
    _argc: i32,
    argv: &mut [TypVal],
    argskip: i32,
    fp: &UserFunc,
) -> i32 {
    if fp.uf_varargs.get() == 0 && (fp.uf_args.borrow().len() as i32) <= argskip {
        return argskip;
    }
    let listarg = &mut argv[argskip as usize];
    tv_list_init_static10(&listarg.value.list());

    let groups: [Option<std::ffi::CString>; NSUBEXP] = match RE_STATE.with(|s| s.rsm.get()) {
        Some(Regsubmatch::Text(m)) => std::array::from_fn(|i| {
            m.group(i)
                .map(|t| cstring_from_bytes(t).unwrap_or_default())
        }),
        _ => Default::default(),
    };
    let mut li = listarg.value.list().lv_first.get();
    for group in groups {
        tv_set_string(&mut li.tv_mut(), group.as_deref());
        li = li.li_next.get();
    }
    argskip + 1
}

/// Substitutes `source` for the match `rmp` found in `subject`, the string that was matched,
/// without its NUL.
pub fn vim_regsub(
    rmp: &Regmatch,
    subject: &[u8],
    source: &[u8],
    expr: Option<&TypVal>,
    dest: &mut [u8],
    flags: i32,
) -> i32 {
    let mut rex = Regexec::for_string(rmp, subject, true);
    rex.rm_startp = rmp.startp;
    rex.rm_endp = rmp.endp;
    rex.bt_magic_wrong = rmp.regprog.bt_magic_wrong();
    vim_regsub_both(&rex, Some(source), expr, dest, flags)
}

/// Substitutes `source` for the match `rmp` found in the current buffer from line `lnum`.
pub fn vim_regsub_multi(
    rmp: &Regmmatch,
    lnum: Linenr,
    source: &[u8],
    dest: &mut [u8],
    flags: i32,
) -> i32 {
    let buf = curbuf();
    let mut rex = Regexec::for_lines(rmp, None, &buf, lnum);
    rex.rmm_startpos = rmp.startpos.map(|pos| pos.unwrap_or(UNSET_LPOS));
    rex.rmm_endpos = rmp.endpos.map(|pos| pos.unwrap_or(UNSET_LPOS));
    rex.bt_magic_wrong = rmp.regprog.bt_magic_wrong();
    vim_regsub_both(&rex, Some(source), None, dest, flags)
}
fn vim_regsub_both(
    rex: &Regexec<'_>,
    source: Option<&[u8]>,
    expr: Option<&TypVal>,
    dest: &mut [u8],
    flags: i32,
) -> i32 {
    let mut no: i32 = -1;
    let mut func_all: Option<CaseConv> = None;
    let mut func_one: Option<CaseConv> = None;
    let mut clnum: Linenr = 0;
    let mut len: i32 = 0;
    let copy = (flags & REGSUB_COPY) != 0;

    if source.is_none() && expr.is_none() {
        emsg(tr(E_NULL));
        return 0;
    }
    if rex.prog_magic_wrong() {
        return 0;
    }
    if RE_STATE.with(|s| s.regsub_nesting.get()) == MAX_REGSUB_NESTING {
        emsg(tr(E_SUBSTITUTE_NESTING_TOO_DEEP));
        return 0;
    }
    let nested = RE_STATE.with(|s| s.regsub_nesting.get());
    let max_dst = dest.len();
    let mut dest_slice = copy.then_some(dest);
    let mut dst_off = 0usize;

    let src_opt = source.map(crate::memory::bytes_until_nul);
    if expr.is_some() || src_opt.is_some_and(|s| s.starts_with(b"\\=")) {
        if copy {
            EVAL_RESULT.with(|er| {
                let mut slot = er[nested].borrow_mut();
                if let Some(ref eval_cstr) = *slot {
                    let eval_bytes = eval_cstr.to_bytes_with_nul();
                    let eval_len = eval_bytes.len() - 1;
                    if eval_len < max_dst {
                        if let Some(ref mut out) = dest_slice {
                            out[..eval_bytes.len()].copy_from_slice(eval_bytes);
                        }
                        dst_off += eval_len;
                        *slot = None;
                    }
                }
            });
        } else {
            EVAL_RESULT.with(|er| *er[nested].borrow_mut() = None);

            let rsm_save = RE_STATE.with(|s| s.rsm.replace(Some(Regsubmatch::of(rex))));

            RE_STATE.with(|s| s.regsub_nesting.set(s.regsub_nesting.get() + (1)));

            if let Some(expr) = expr {
                let mut argv: [TypVal; 2] = [const { TypVal::new() }; 2];
                let mut rettv: TypVal = TypVal::new();
                let match_list_id = tv_list_alloc_static();

                rettv.value = crate::eval::typval::TvValue::NULL_STRING;

                argv[0].value = crate::eval::typval::TvValue::List(match_list_id.clone());
                let mut funcexe = Funcexe {
                    fe_fill_submatch: true,
                    fe_firstline: 0,
                    fe_lastline: 0,
                    fe_doesrange: false,
                    fe_evaluate: true,
                    fe_partial: crate::eval::typval::PartialId::NULL,
                    fe_selfdict: crate::eval::typval::DictId::NULL,
                    fe_basetv: None,
                    fe_found_var: false,
                };
                if expr.var_type() == VarType::VarFunc {
                    let s_bytes = expr.value.string_bytes();
                    call_func(s_bytes, -1, &mut rettv, 1, &mut argv, &mut funcexe);
                } else if expr.var_type() == VarType::VarPartial {
                    let partial = expr.value.partial();
                    let s = partial_name(partial.opt());
                    funcexe.fe_partial = partial.clone();
                    call_func(s.to_bytes(), -1, &mut rettv, 1, &mut argv, &mut funcexe);
                }
                if rettv.var_type() == VarType::VarUnknown {
                    EVAL_RESULT.with(|er| *er[nested].borrow_mut() = None);
                } else {
                    let mut buf = [0u8; NUMBUFLEN];
                    if let Some(s) = tv_get_string_buf_chk(&rettv, &mut buf) {
                        let owned = s.to_owned();
                        EVAL_RESULT.with(|er| *er[nested].borrow_mut() = Some(owned));
                    }
                }
                tv_clear(&mut rettv);
                if let Some(match_list) = match_list_id.opt() {
                    tv_list_free_contents(match_list);
                    unref_var_list(match_list);
                }
            } else if let Some(src) = src_opt {
                let val = eval_to_string(src.get(2..).unwrap_or(&[]), true, false);
                EVAL_RESULT.with(|er| *er[nested].borrow_mut() = val);
            }
            RE_STATE.with(|s| s.regsub_nesting.set(s.regsub_nesting.get() - (1)));

            let eval_cstr_opt = EVAL_RESULT.with(|er| er[nested].borrow_mut().take());
            if let Some(eval_cstr) = eval_cstr_opt {
                let mut eval_bytes = eval_cstr.into_bytes_with_nul();
                let mut had_backslash = false;
                let eval_len = eval_bytes.len().saturating_sub(1);
                let mut s_idx = 0usize;
                while s_idx < eval_len {
                    if eval_bytes[s_idx] == NL as u8 && !rex.reg_line_lbr {
                        eval_bytes[s_idx] = CAR as u8;
                    } else if eval_bytes[s_idx] == b'\\' && s_idx + 1 < eval_len {
                        s_idx += 1;
                        if eval_bytes[s_idx] == NL as u8 && !rex.reg_line_lbr {
                            eval_bytes[s_idx] = CAR as u8;
                        }
                        had_backslash = true;
                    }
                    s_idx += crate::mbyte::utfc_slice2len(&eval_bytes[s_idx..eval_len]);
                }
                let final_cstr = if had_backslash && (flags & REGSUB_BACKSLASH) != 0 {
                    vim_strsave_escaped(&eval_bytes, b"\\")
                } else {
                    CStr::from_bytes_with_nul(&eval_bytes)
                        .unwrap_or_default()
                        .to_owned()
                };
                dst_off += final_cstr.to_bytes().len();
                EVAL_RESULT.with(|er| *er[nested].borrow_mut() = Some(final_cstr));
            }

            RE_STATE.with(|s| s.rsm.set(rsm_save));
        }
    } else if let Some(src_bytes) = src_opt {
        let mut idx = 0usize;
        while idx < src_bytes.len() {
            let mut c = src_bytes[idx] as i32;
            idx += 1;
            if c == b'&' as i32 && (flags & REGSUB_MAGIC) != 0 {
                no = 0;
            } else if c == b'\\' as i32
                && let Some(&next_b) = src_bytes.get(idx)
            {
                if next_b == b'&' && (flags & REGSUB_MAGIC) == 0 {
                    idx += 1;
                    no = 0;
                } else if next_b.is_ascii_digit() {
                    no = (next_b - b'0') as i32;
                    idx += 1;
                } else if matches!(next_b, b'u' | b'U' | b'l' | b'L' | b'e' | b'E') {
                    idx += 1;
                    match next_b {
                        b'u' => {
                            func_one = Some(CaseConv::Upper);
                            continue;
                        }
                        b'U' => {
                            func_all = Some(CaseConv::Upper);
                            continue;
                        }
                        b'l' => {
                            func_one = Some(CaseConv::Lower);
                            continue;
                        }
                        b'L' => {
                            func_all = Some(CaseConv::Lower);
                            continue;
                        }
                        b'e' | b'E' => {
                            func_one = None;
                            func_all = None;
                            continue;
                        }
                        _ => {}
                    }
                }
            }
            if no < 0 {
                if c == i32::from(K_SPECIAL) && idx + 2 <= src_bytes.len() {
                    if let Some(ref mut out) = dest_slice {
                        if dst_off + 3 > max_dst {
                            iemsg(c"vim_regsub_both(): not enough space");
                            return 0;
                        }
                        out[dst_off] = c as u8;
                        out[dst_off + 1] = src_bytes[idx];
                        out[dst_off + 2] = src_bytes[idx + 1];
                    }
                    dst_off += 3;
                    idx += 2;
                    continue;
                }

                if c == b'\\' as i32
                    && let Some(&next_b) = src_bytes.get(idx)
                {
                    match next_b {
                        b'r' => {
                            c = CAR;
                            idx += 1;
                        }
                        b'n' => {
                            c = NL;
                            idx += 1;
                        }
                        b't' => {
                            c = TAB;
                            idx += 1;
                        }
                        b'b' => {
                            c = CTRL_H_CONST;
                            idx += 1;
                        }
                        _ => {
                            if (flags & REGSUB_BACKSLASH) != 0 {
                                if let Some(ref mut out) = dest_slice {
                                    if dst_off + 1 > max_dst {
                                        iemsg(c"vim_regsub_both(): not enough space");
                                        return 0;
                                    }
                                    out[dst_off] = b'\\';
                                }
                                dst_off += 1;
                            }
                            c = next_b as i32;
                            idx += 1;
                        }
                    }
                } else {
                    c = crate::mbyte::utf_slice2char(&src_bytes[idx - 1..]);
                }

                let cc = if let Some(f) = func_one.take() {
                    f.apply(c)
                } else if let Some(f) = func_all {
                    f.apply(c)
                } else {
                    c
                };

                let cur_slice = &src_bytes[idx - 1..];
                let totlen = crate::mbyte::utfc_slice2len(cur_slice);
                let charlen = utf_char2len(cc) as usize;

                if let Some(ref mut out) = dest_slice {
                    if dst_off + charlen > max_dst {
                        iemsg(c"vim_regsub_both(): not enough space");
                        return 0;
                    }
                    crate::mbyte::utf_char2slice(cc, &mut out[dst_off..]);
                }
                dst_off = (dst_off + charlen).wrapping_sub(1);
                let clen = crate::mbyte::utf_slice2len(cur_slice);

                if clen < totlen {
                    let extra_len = totlen - clen;
                    if let Some(ref mut out) = dest_slice {
                        if dst_off + extra_len > max_dst {
                            iemsg(c"vim_regsub_both(): not enough space");
                            return 0;
                        }
                        out[dst_off + 1..dst_off + 1 + extra_len]
                            .copy_from_slice(&cur_slice[clen..totlen]);
                    }
                    dst_off += extra_len;
                }
                idx += totlen - 1;
                dst_off = dst_off.wrapping_add(1);
            } else {
                // The line the submatch is copied from, and the offset in it.
                let mut line: Option<RegLine<'_>> = None;
                let mut pos = 0usize;
                if rex.reg_multi() {
                    let spos = rex.rmm_startpos[no as usize];
                    let epos = rex.rmm_endpos[no as usize];
                    clnum = spos.lnum;
                    if clnum >= 0 && epos.lnum >= 0 {
                        line = rex.getline(clnum);
                        pos = spos.col as usize;
                        len = if epos.lnum == clnum {
                            epos.col - spos.col
                        } else {
                            rex.getline_len(clnum) - spos.col
                        };
                    }
                } else if let (Some(start), Some(end)) =
                    (rex.rm_startp[no as usize], rex.rm_endp[no as usize])
                {
                    line = Some(rex.line.clone());
                    pos = start;
                    len = end.saturating_sub(start) as i32;
                }
                if let Some(mut line) = line {
                    loop {
                        if len == 0 {
                            if rex.reg_multi() {
                                let epos = rex.rmm_endpos[no as usize];
                                if epos.lnum == clnum {
                                    break;
                                }
                                if let Some(ref mut out) = dest_slice {
                                    if dst_off + 1 > max_dst {
                                        iemsg(c"vim_regsub_both(): not enough space");
                                        return 0;
                                    }
                                    out[dst_off] = CAR as u8;
                                }
                                dst_off += 1;
                                clnum += 1;
                                line = rex.getline(clnum).unwrap_or_default();
                                pos = 0;
                                len = if epos.lnum == clnum {
                                    epos.col
                                } else {
                                    rex.getline_len(clnum)
                                };
                            } else {
                                break;
                            }
                            continue;
                        }
                        let s = line.bytes().get(pos..).unwrap_or(&[]);
                        let Some(&s_b0) = s.first() else {
                            if copy {
                                iemsg(tr(E_RE_DAMG));
                            }
                            return (dst_off + 1) as i32;
                        };
                        if (flags & REGSUB_BACKSLASH) != 0 && (s_b0 == CAR as u8 || s_b0 == b'\\') {
                            if let Some(ref mut out) = dest_slice {
                                if dst_off + 2 > max_dst {
                                    iemsg(c"vim_regsub_both(): not enough space");
                                    return 0;
                                }
                                out[dst_off] = b'\\';
                                out[dst_off + 1] = s_b0;
                            }
                            dst_off += 2;
                        } else {
                            c = utf_ptr2char(s);
                            let cc = if let Some(f) = func_one.take() {
                                f.apply(c)
                            } else if let Some(f) = func_all {
                                f.apply(c)
                            } else {
                                c
                            };

                            let l = utf_ptr2len(s) - 1;
                            pos += l as usize;
                            len -= l;
                            let charlen = utf_char2len(cc) as usize;
                            if let Some(ref mut out) = dest_slice {
                                if dst_off + charlen > max_dst {
                                    iemsg(c"vim_regsub_both(): not enough space");
                                    return 0;
                                }
                                crate::mbyte::utf_char2slice(cc, &mut out[dst_off..]);
                            }
                            dst_off += charlen;
                        }
                        pos += 1;
                        len -= 1;
                    }
                }
                no = -1;
            }
        }
    }
    if let Some(ref mut out) = dest_slice {
        out[dst_off] = NUL;
    }
    (dst_off + 1) as i32
}

/// The text group `no` of the match being substituted matched, for submatch().
pub fn reg_submatch(no: i32) -> Option<std::ffi::CString> {
    let no = usize::try_from(no).ok()?;
    match RE_STATE.with(|s| s.rsm.get())? {
        Regsubmatch::Text(m) => m.group(no).and_then(cstring_from_bytes),
        Regsubmatch::Lines(m) => {
            let (Some(start), Some(end)) = (m.startpos[no], m.endpos[no]) else {
                return None;
            };
            let mut lnum = start.lnum;
            let end_lnum = end.lnum;
            let first_line = m.getline(lnum)?;
            let first_line = first_line.bytes();
            let scol = (start.col as usize).min(first_line.len());
            let ecol = end.col as usize;
            let mut buf: Vec<u8> = Vec::new();
            if end_lnum == lnum {
                let end_idx = ecol.min(first_line.len());
                buf.extend_from_slice(&first_line[scol..end_idx.max(scol)]);
            } else {
                buf.extend_from_slice(&first_line[scol..]);
                buf.push(b'\n');
                lnum += 1;
                while lnum < end_lnum {
                    buf.extend_from_slice(m.getline(lnum).unwrap_or_default().bytes());
                    buf.push(b'\n');
                    lnum += 1;
                }
                let last_line = m.getline(lnum).unwrap_or_default();
                let last_line = last_line.bytes();
                buf.extend_from_slice(&last_line[..ecol.min(last_line.len())]);
            }
            Some(crate::memory::bytes_to_cstring(&buf))
        }
    }
}

/// The lines group `no` of the match being substituted matched, for submatch(no, 1).
pub fn reg_submatch_list(no: i32) -> Option<crate::eval::typval::ListId> {
    let no = usize::try_from(no).ok()?;
    let list = match RE_STATE.with(|s| s.rsm.get())? {
        Regsubmatch::Text(m) => {
            let text = m.group(no)?;
            let list = tv_list_alloc(1);
            crate::eval::typval::tv_list_append_bytes(&list, text);
            list
        }
        Regsubmatch::Lines(m) => {
            let (Some(start), Some(end)) = (m.startpos[no], m.endpos[no]) else {
                return None;
            };
            let (slnum, elnum) = (start.lnum, end.lnum);
            let (scol, ecol) = (start.col, end.col);

            let list = tv_list_alloc((elnum - slnum + 1) as isize);

            let first_line = m.getline(slnum).unwrap_or_default();
            let first_line = first_line.bytes();
            let s = &first_line[(scol as usize).min(first_line.len())..];
            if slnum == elnum {
                crate::eval::typval::tv_list_append_bytes(
                    &list,
                    &s[..((ecol - scol) as usize).min(s.len())],
                );
            } else {
                crate::eval::typval::tv_list_append_bytes(&list, s);
                for i in 1..elnum - slnum {
                    let line = m.getline(slnum + i).unwrap_or_default();
                    crate::eval::typval::tv_list_append_bytes(&list, line.bytes());
                }
                let last_line = m.getline(elnum).unwrap_or_default();
                let last_line = last_line.bytes();
                crate::eval::typval::tv_list_append_bytes(
                    &list,
                    &last_line[..(ecol as usize).min(last_line.len())],
                );
            }
            list
        }
    };

    tv_list_ref(Some(&list));
    Some(list)
}
pub(super) fn get_char_class_bits(slice: &[u8], newl: &mut bool) -> i32 {
    let mut config: i32 = 0;
    *newl = false;
    let Some((&b']', body)) = slice.split_last() else {
        return -1;
    };
    let len = body.len();
    let mut i = 0;
    if body.first() == Some(&b'^') {
        config |= CLASS_NOT;
        i += 1;
    }

    while i < len {
        if i + 2 < len && body[i + 1] == b'-' {
            match body[i] {
                b'0' => {
                    if body[i + 2] == b'9' {
                        config |= CLASS_O9;
                    } else if body[i + 2] == b'7' {
                        config |= CLASS_O7;
                    } else {
                        return -1;
                    }
                }
                b'a' => {
                    if body[i + 2] == b'z' {
                        config |= CLASS_AZ_CONST;
                    } else if body[i + 2] == b'f' {
                        config |= CLASS_AF_CONST;
                    } else {
                        return -1;
                    }
                }
                b'A' => {
                    if body[i + 2] == b'Z' {
                        config |= CLASS_AZ;
                    } else if body[i + 2] == b'F' {
                        config |= CLASS_AF;
                    } else {
                        return -1;
                    }
                }
                _ => return -1,
            }
            i += 3;
        } else if i + 1 < len && body[i] == b'\\' && body[i + 1] == b'n' {
            *newl = true;
            i += 2;
        } else if body[i] == b'_' {
            config |= CLASS_UNDERSCORE;
            i += 1;
        } else if body[i] == b'\n' {
            *newl = true;
            i += 1;
        } else {
            return -1;
        }
    }

    if i != len {
        return -1;
    }
    if (config & (CLASS_AZ_CONST | CLASS_AZ)) != 0 {
        return -1;
    }
    config
}

pub(super) fn regcomp_start(expr: &[u8], re_flags: i32) {
    RE_STATE.with(|re_s| {
        initchr(expr);
        if (re_flags & RE_MAGIC) != 0 {
            re_s.reg_magic.set(MAGIC_ON);
        } else {
            re_s.reg_magic.set(MAGIC_OFF);
        }
        re_s.reg_string.set(re_flags & RE_STRING);
        re_s.reg_strict.set(re_flags & RE_STRICT);
        get_cpo_flags();

        re_s.num_complex_braces.set(0);
        re_s.regnpar.set(1);
        RE_STATE.with(|s| {
            for c in &s.had_endbrace {
                c.set(0);
            }
        });
        re_s.regnzpar.set(1);
        re_s.re_has_z.set(0);
        re_s.regsize.set(0);
        re_s.reg_toolong.set(0);
        re_s.bt_reg_parse_depth.set(0);
        re_s.regflags.set(0);
        re_s.had_eol.set(false);
    })
}

pub(super) fn seen_endbrace(refnum: i32) -> bool {
    if RE_STATE.with(|s| s.had_endbrace[refnum as usize].get()) == 0 {
        let has_lookbehind = re_st().with_regparse_bytes(|rb| {
            rb.windows(3)
                .any(|w| w[0] == b'@' && w[1] == b'<' && (w[2] == b'!' || w[2] == b'='))
        });
        if !has_lookbehind {
            emsg(tr(c"E65: Illegal back reference"));
            msg_state().rc_did_emsg.set(true);
            return false;
        }
    }
    true
}

pub(super) fn coll_get_char() -> i32 {
    let mut nr: i64 = -1;
    let ch = re_st().regparse_byte();
    RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() + (1)));
    match ch {
        b'd' => nr = getdecchrs(),
        b'o' => nr = getoctchrs(),
        b'x' => nr = gethexchrs(2),
        b'u' => nr = gethexchrs(4),
        b'U' => nr = gethexchrs(8),
        _ => {}
    }
    if nr < 0 {
        RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() - (1)));
        nr = i64::from(b'\\');
    }
    if nr > i64::from(i32::MAX) {
        nr = i64::from(i32::MAX);
    }
    nr as i32
}

/// The keyword characters of the current buffer, which a pattern is compiled for.
pub(super) fn curbuf_chartab() -> [u64; 4] {
    workspace_state()
        .curbuf
        .get()
        .opt()
        .map_or([0; 4], |buf| buf.b_chartab.get())
}

pub fn vim_regcomp_had_eol() -> bool {
    RE_STATE.with(|s| s.had_eol.get())
}

#[inline]
pub fn vim_regcomp_cached(expr_arg: &[u8], re_flags: i32) -> Option<Box<Regprog>> {
    if RE_STATE.with(|s| s.reg_do_extmatch.get()) != 0 || opt_vars().p_verbose.get() != 0 {
        return None;
    }
    let mut expr_bytes = crate::memory::bytes_until_nul(expr_arg);
    let mut engine = opt_vars().p_re.get() as i32;
    if expr_bytes.starts_with(b"\\%#=") {
        let newengine = expr_bytes.get(4).copied().unwrap_or(0).wrapping_sub(b'0') as i32;
        if newengine == AUTOMATIC_ENGINE
            || newengine == BACKTRACKING_ENGINE
            || newengine == NFA_ENGINE
        {
            engine = newengine;
            expr_bytes = &expr_bytes[5..];
        } else {
            return None;
        }
    }
    if expr_bytes.contains(&b'~') || expr_bytes.windows(2).any(|w| w == b"[:") {
        return None;
    }
    let cpo_lit = crate::strings::vim_haschr(&opt_vars().p_cpo.as_cstr(), K_CPO_LITERAL);
    let chartab = curbuf_chartab();
    let cached = LAST_REGPROG.with(|cell| {
        let borrow = cell.borrow();
        if let Some(ref c) = *borrow
            && std::rc::Rc::strong_count(&c.prog.0) == 1
            && c.re_flags == re_flags
            && c.engine == engine
            && c.cpo_lit == cpo_lit
            && c.chartab == chartab
            && c.expr.as_slice() == expr_bytes
        {
            Some((c.prog.clone(), c.had_eol))
        } else {
            None
        }
    });
    let (prog, had_eol) = cached?;
    RE_STATE.with(|re_s| {
        re_s.regexp_engine.set(engine);
        re_s.had_eol.set(had_eol);
    });
    Some(Box::new(prog))
}

/// Compile a regular expression into internal code.
pub fn vim_regcomp(expr_arg: &[u8], re_flags: i32) -> Option<Box<Regprog>> {
    RE_STATE.with(|re_s| {
    let mut expr_bytes = crate::memory::bytes_until_nul(expr_arg);

    re_s.regexp_engine.set(opt_vars().p_re.get() as i32);

    if expr_bytes.starts_with(b"\\%#=") {
        let newengine = expr_bytes.get(4).copied().unwrap_or(0).wrapping_sub(b'0') as i32;
        if newengine == AUTOMATIC_ENGINE
            || newengine == BACKTRACKING_ENGINE
            || newengine == NFA_ENGINE
        {
            re_s.regexp_engine.set(newengine);
            expr_bytes = &expr_bytes[5..];
        } else {
            emsg(tr(
                c"E864: \\%#= can only be followed by 0, 1, or 2. The automatic engine will be used ",
            ));
            re_s.regexp_engine.set(AUTOMATIC_ENGINE);
        }
    }

    let engine = re_s.regexp_engine.get();
    let cpo_lit = crate::strings::vim_haschr(&opt_vars().p_cpo.as_cstr(), K_CPO_LITERAL);
    let chartab = curbuf_chartab();
    let can_cache = re_s.reg_do_extmatch.get() == 0
        && opt_vars().p_verbose.get() == 0
        && !expr_bytes.contains(&b'~')
        && !expr_bytes.windows(2).any(|w| w == b"[:");
    if can_cache {
        let cached = LAST_REGPROG.with(|cell| {
            let borrow = cell.borrow();
            if let Some(ref c) = *borrow
                && std::rc::Rc::strong_count(&c.prog.0) == 1
                && c.re_flags == re_flags
                && c.engine == engine
                && c.cpo_lit == cpo_lit
                && c.chartab == chartab
                && c.expr.as_slice() == expr_bytes
            {
                Some((c.prog.clone(), c.had_eol))
            } else {
                None
            }
        });
        if let Some((prog, had_eol)) = cached {
            re_s.had_eol.set(had_eol);
            return Some(Box::new(prog));
        }
    }

    let called_emsg_before = msg_state().called_emsg.get();
    let mut prog_opt = if engine != BACKTRACKING_ENGINE {
        nfa_regcomp(
            expr_bytes,
            re_flags
                + if engine == AUTOMATIC_ENGINE {
                    RE_AUTO
                } else {
                    0
                },
        )
        .map(|mut nfa| {
            nfa.re_engine = re_s.regexp_engine.get() as u32;
            Regprog::new(
                nfa.regflags,
                re_s.regexp_engine.get() as u32,
                re_flags as u32,
                RegProgEngine::Nfa(nfa),
            )
        })
    } else {
        bt_regcomp(expr_bytes, re_flags).map(|bt| {
            Regprog::new(
                bt.regflags,
                re_s.regexp_engine.get() as u32,
                re_flags as u32,
                RegProgEngine::Bt(bt),
            )
        })
    };

    if prog_opt.is_none()
        && re_s.regexp_engine.get() == AUTOMATIC_ENGINE
        && msg_state().called_emsg.get() == called_emsg_before
    {
        re_s.regexp_engine.set(BACKTRACKING_ENGINE);
        report_re_switch(expr_bytes);
        prog_opt = bt_regcomp(expr_bytes, re_flags).map(|bt| {
            Regprog::new(
                bt.regflags,
                re_s.regexp_engine.get() as u32,
                re_flags as u32,
                RegProgEngine::Bt(bt),
            )
        });
    }

    if let Some(ref prog) = prog_opt
        && can_cache
        && msg_state().called_emsg.get() == called_emsg_before
    {
        LAST_REGPROG.with(|cell| {
            *cell.borrow_mut() = Some(CachedRegprog {
                expr: expr_bytes.to_vec(),
                re_flags,
                engine,
                cpo_lit,
                chartab,
                had_eol: re_s.had_eol.get(),
                prog: prog.clone(),
            });
        });
    }

    prog_opt.map(Box::new)
    })
}

#[inline(always)]
pub fn vim_regcomp_box(expr_arg: &[u8], re_flags: i32) -> Option<Box<Regprog>> {
    vim_regcomp(expr_arg, re_flags)
}

pub fn free_regexp_stuff() {
    free_bt_regstack();
    LAST_REGPROG.with(|c| *c.borrow_mut() = None);
    REG_PREV_SUB.with(|c| *c.borrow_mut() = None);
    POST_VEC.with(|c| *c.borrow_mut() = Vec::new());
}

fn report_re_switch(pat: &[u8]) {
    if opt_vars().p_verbose.get() > 0 {
        verbose_enter();
        msg_puts(tr(c"Switching to backtracking RE engine for pattern: "));
        msg_puts(&crate::memory::bytes_to_cstring(pat));
        verbose_leave();
    }
}

fn exec_regmatch_nl(rmp: &mut Regmatch, line: &[u8], col: Colnr, nl: bool) -> i32 {
    let mut rex = Regexec::for_string(rmp, line, nl);
    let res = {
        let prog = rmp.regprog.opt().unwrap();
        let eng = prog.0.engine.borrow();
        match &*eng {
            RegProgEngine::Bt(bt) => bt_regexec_both(bt, &mut rex, col, None, None),
            RegProgEngine::Nfa(nfa) => nfa_regexec_both(nfa, &mut rex, col, None, None),
        }
    };
    if res > 0 {
        rex.save_match(rmp);
    } else {
        rmp.rm_matchcol = rex.rm_matchcol;
    }
    res
}
fn exec_regmmatch_multi(
    rmp: &mut Regmmatch,
    win: Option<&WinInfoInfo>,
    buf: &FileBuf,
    lnum: Linenr,
    mut col: Colnr,
    tm: Option<Proftime>,
    timed_out: Option<&mut i32>,
) -> i32 {
    if lnum >= 1 && lnum <= buf.b_ml.ml_line_count.get() {
        let pre_action = {
            let prog = rmp.regprog.opt().unwrap();
            let eng = prog.0.engine.borrow();
            if let RegProgEngine::Nfa(nfa) = &*eng {
                if nfa.reganch && col > 0 {
                    Some(None)
                } else if nfa.regstart != NUL as i32 {
                    let reg_ic = if (nfa.regflags & RF_ICASE) != 0 {
                        true
                    } else if (nfa.regflags & RF_NOICASE) != 0 {
                        false
                    } else {
                        rmp.rmm_ic
                    };
                    let reg_icombine = (nfa.regflags & RF_ICOMBINE) != 0;
                    let pre = crate::memline::with_ml_buf_line(buf, lnum, |line_bytes| {
                        let start_off = (col as usize).min(line_bytes.len());
                        let off = cstrchr_ic(&line_bytes[start_off..], nfa.regstart, reg_ic)?;
                        let mut mcol = col + off as Colnr;
                        if let Some(ref mt) = nfa.match_text
                            && !mt.as_bytes().is_empty()
                            && !reg_icombine
                        {
                            let end_col = find_match_text_in_bytes(
                                line_bytes,
                                &mut mcol,
                                nfa.regstart,
                                mt.as_bytes(),
                                reg_ic,
                            );
                            return Some((mcol, Some(end_col)));
                        }
                        Some((mcol, None))
                    });
                    match pre {
                        None => Some(None),
                        Some((mcol, Some(None))) => {
                            rmp.rmm_matchcol = mcol;
                            Some(None)
                        }
                        Some((mcol, Some(Some(end_col)))) => Some(Some((mcol, end_col))),
                        Some((mcol, None)) => {
                            col = mcol;
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };
        match pre_action {
            Some(None) => return 0,
            Some(Some((mcol, end_col))) => {
                let mut rex = Regexec::for_lines(rmp, win, buf, lnum);
                RE_STATE.with(|s| s.re_extmatch_out.set(None));
                rex.need_clear_subexpr = true;
                rex.cleanup_subexpr();
                rex.rmm_startpos[0] = Lpos { lnum: 0, col: mcol };
                rex.rmm_endpos[0] = Lpos {
                    lnum: 0,
                    col: end_col,
                };
                rex.rmm_matchcol = mcol;
                rex.save_mmatch(rmp);
                return 1;
            }
            None => {}
        }
    }

    let mut rex = Regexec::for_lines(rmp, win, buf, lnum);
    let res = {
        let prog = rmp.regprog.opt().unwrap();
        let eng = prog.0.engine.borrow();
        match &*eng {
            RegProgEngine::Bt(bt) => bt_regexec_both(bt, &mut rex, col, tm, timed_out),
            RegProgEngine::Nfa(nfa) => nfa_regexec_both(nfa, &mut rex, col, tm, timed_out),
        }
    };
    if res > 0 {
        rex.save_mmatch(rmp);
    } else {
        rmp.rmm_matchcol = rex.rmm_matchcol;
    }
    res
}

fn vim_regexec_string(rmp: &mut Regmatch, line: &[u8], col: Colnr, nl: bool) -> bool {
    if rmp.regprog.re_in_use() {
        emsg(tr(E_RECURSIVE));
        return false;
    }
    rmp.regprog.set_re_in_use(true);

    let mut result = exec_regmatch_nl(rmp, line, col, nl);
    rmp.regprog.set_re_in_use(false);

    if rmp.regprog.re_engine() == AUTOMATIC_ENGINE as u32 && result == NFA_TOO_EXPENSIVE {
        let save_p_re = opt_vars().p_re.get();
        let re_flags = rmp.regprog.re_flags() as i32;
        let pat_owned = rmp.regprog.nfa_pattern();

        opt_vars().p_re.set(OptInt::from(BACKTRACKING_ENGINE));
        report_re_switch(pat_owned.as_bytes());
        if let Some(new_prog) = vim_regcomp(pat_owned.as_bytes(), re_flags) {
            rmp.regprog.replace_from(&new_prog);
            rmp.regprog.set_re_in_use(true);
            result = exec_regmatch_nl(rmp, line, col, nl);
            rmp.regprog.set_re_in_use(false);
        }

        opt_vars().p_re.set(save_p_re);
    }

    result > 0
}

/// Matches `prog` in `line`, a string without its NUL, from byte offset `col`.
pub fn vim_regexec_prog(
    prog: Option<&Regprog>,
    ignore_case: bool,
    line: &[u8],
    col: Colnr,
) -> bool {
    let mut regmatch = Regmatch {
        regprog: RegprogPtr::from_ref(prog),
        rm_ic: ignore_case,
        ..Regmatch::new()
    };
    vim_regexec_string(&mut regmatch, line, col, false)
}

/// Matches `rmp` in `line`, a string without its NUL, from byte offset `col`.
pub fn vim_regexec(rmp: &mut Regmatch, line: &[u8], col: Colnr) -> bool {
    vim_regexec_string(rmp, line, col, false)
}

/// Like [`vim_regexec`], but a "\n" in `line` matches a line break.
pub fn vim_regexec_nl(rmp: &mut Regmatch, line: &[u8], col: Colnr) -> bool {
    vim_regexec_string(rmp, line, col, true)
}

pub fn vim_regexec_multi(
    rmp: &mut Regmmatch,
    win: Option<&WinInfoInfo>,
    buf: &FileBuf,
    lnum: Linenr,
    col: Colnr,
    tm: Option<Proftime>,
    mut timed_out: Option<&mut i32>,
) -> i32 {
    if rmp.regprog.re_in_use() {
        emsg(tr(E_RECURSIVE));
        return 0;
    }
    rmp.regprog.set_re_in_use(true);

    let mut result = exec_regmmatch_multi(rmp, win, buf, lnum, col, tm, timed_out.as_deref_mut());
    rmp.regprog.set_re_in_use(false);

    if rmp.regprog.re_engine() == AUTOMATIC_ENGINE as u32 && result == NFA_TOO_EXPENSIVE {
        let save_p_re = opt_vars().p_re.get();
        let re_flags = rmp.regprog.re_flags() as i32;
        let pat_owned = rmp.regprog.nfa_pattern();

        opt_vars().p_re.set(OptInt::from(BACKTRACKING_ENGINE));

        report_re_switch(pat_owned.as_bytes());
        RE_STATE.with(|s| s.reg_do_extmatch.set(REX_ALL));
        let new_prog = vim_regcomp(pat_owned.as_bytes(), re_flags);
        RE_STATE.with(|s| s.reg_do_extmatch.set(0));

        if let Some(new_prog) = new_prog {
            rmp.regprog.replace_from(&new_prog);

            rmp.regprog.set_re_in_use(true);
            result = exec_regmmatch_multi(rmp, win, buf, lnum, col, tm, timed_out);
            rmp.regprog.set_re_in_use(false);
        }

        opt_vars().p_re.set(save_p_re);
    }

    if result <= 0 {
        RE_STATE.with(|s| s.re_extmatch_out.set(None));
    }

    if result <= 0 { 0 } else { result }
}

pub(crate) use self::nfa::{find_match_text_in_bytes, nfa_match_line_slice};

/// Prepares a worker thread for matching with the global character classes and the
/// 'maxmempattern' value of the main thread.
#[inline]
pub(crate) fn init_worker_regex_env(isf_chartab: [u64; 4], isi_chartab: [u64; 4], p_mmp: OptInt) {
    crate::charset::set_global_chartabs(isf_chartab, isi_chartab);
    opt_vars().p_mmp.set(p_mmp);
}
#[inline]
pub(crate) fn with_pure_nfa_prog<R>(
    regprog: &RegprogPtr,
    f: impl FnOnce(&NfaRegprog) -> R,
) -> Option<R> {
    let prog = regprog.opt()?;
    let eng = prog.0.engine.borrow();
    match &*eng {
        RegProgEngine::Nfa(nfa) if nfa.is_pure_single_line() => Some(f(nfa)),
        _ => None,
    }
}

impl RegexpState {
    #[inline(always)]
    pub fn with_regparse_bytes<R>(&self, f: impl FnOnce(&[u8]) -> R) -> R {
        REGPARSE_BASE.with(|cell| {
            let base = cell.borrow();
            f(base.get(self.regparse_off.get()..).unwrap_or(&[]))
        })
    }
    #[inline(always)]
    pub fn regparse_byte(&self) -> u8 {
        self.with_regparse_bytes(|b| b.first().copied().unwrap_or(0))
    }
    #[inline(always)]
    pub fn regparse_byte_at(&self, idx: usize) -> u8 {
        self.with_regparse_bytes(|b| b.get(idx).copied().unwrap_or(0))
    }
    #[inline(always)]
    pub fn regparse_mb_adv(&self) {
        let step = self.with_regparse_bytes(|b| utfc_ptr2len(b)) as usize;
        self.regparse_off.set(self.regparse_off.get() + step);
    }
    #[inline(always)]
    pub fn regparse_prev_char(&self) {
        let off = self.regparse_off.get();
        let step = REGPARSE_BASE.with(|cell| {
            let base = cell.borrow();
            1 + crate::mbyte::utf_head_off_slice(&base, off - 1) as usize
        });
        self.regparse_off.set(off - step);
    }
    /// The length of the composing characters that follow the character of `char_len` bytes just
    /// read: the rest of the character, as `utfc_ptr2len()` counts it.
    pub fn regparse_composing_len(&self, char_len: usize) -> usize {
        REGPARSE_BASE.with(|cell| {
            let base = cell.borrow();
            let start = self.regparse_off.get().saturating_sub(char_len);
            (utfc_ptr2len(base.get(start..).unwrap_or_default()) as usize).saturating_sub(char_len)
        })
    }
    #[inline(always)]
    pub fn regparse_get_char_class(&self) -> i32 {
        let (res, adv) = self.with_regparse_bytes(|bytes| {
            let mut rem = bytes;
            let res = get_char_class(&mut rem);
            (res, bytes.len() - rem.len())
        });
        self.regparse_off.set(self.regparse_off.get() + adv);
        res
    }
    #[inline(always)]
    pub fn regparse_get_equi_class(&self) -> i32 {
        let (res, adv) = self.with_regparse_bytes(|bytes| {
            let mut rem = bytes;
            let res = get_equi_class(&mut rem);
            (res, bytes.len() - rem.len())
        });
        self.regparse_off.set(self.regparse_off.get() + adv);
        res
    }
    #[inline(always)]
    pub fn regparse_get_coll_element(&self) -> i32 {
        let (res, adv) = self.with_regparse_bytes(|bytes| {
            let mut rem = bytes;
            let res = get_coll_element(&mut rem);
            (res, bytes.len() - rem.len())
        });
        self.regparse_off.set(self.regparse_off.get() + adv);
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_helpers_round_trip() {
        let c = b'*' as i32;
        let m = magic_fn(c);
        assert!(is_magic_fn(m));
        assert_eq!(un_magic(m), c);
        assert_eq!(no_magic(m), c);
        assert_eq!(no_magic(c), c);
        assert_eq!(toggle_magic(c), m);
        assert_eq!(toggle_magic(m), c);
    }

    #[test]
    fn re_multi_type_classifies_operators() {
        assert_eq!(re_multi_type(magic_fn(b'*' as i32)), MULTI_MULT);
        assert_eq!(re_multi_type(magic_fn(b'+' as i32)), MULTI_MULT);
        assert_eq!(re_multi_type(magic_fn(b'{' as i32)), MULTI_MULT);
        assert_eq!(re_multi_type(magic_fn(b'=' as i32)), MULTI_ONE);
        assert_eq!(re_multi_type(magic_fn(b'?' as i32)), MULTI_ONE);
        assert_eq!(re_multi_type(magic_fn(b'@' as i32)), MULTI_ONE);
        assert_eq!(re_multi_type(b'*' as i32), NOT_MULTI);
    }

    #[test]
    fn get_char_class_bits_recognizes_digit_and_hex() {
        let mut newl = false;
        let digits = b"0-9]";
        assert_eq!(get_char_class_bits(digits, &mut newl), CLASS_O9);
        assert!(!newl);

        let hex_nl = b"^0-9a-fA-F\\n]";
        assert_eq!(
            get_char_class_bits(hex_nl, &mut newl),
            CLASS_NOT | CLASS_O9 | CLASS_AF_CONST | CLASS_AF
        );
        assert!(newl);
    }

    #[test]
    fn byte_slice_skip_and_exec_work_without_trailing_nul() {
        let _guard = crate::globals::TEST_GLOBAL_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let raw_cmd = b"foo\\/bar/baz/g";
        let rem = skip_regexp(&raw_cmd[..12], b'/' as i32, 1);
        assert_eq!(rem, b"/baz");
        assert_eq!(
            skip_regexp_err(&raw_cmd[..12], b'/' as i32, 1),
            Some(&b"/baz"[..])
        );

        let pat_buf = b"ba[rz]EXTRA";
        let line_buf = b"xxbarzz";
        let mut rm = Regmatch::new();
        rm.set_prog(vim_regcomp(&pat_buf[..6], RE_MAGIC + RE_STRING));
        assert!(!rm.regprog.is_null());
        assert!(vim_regexec(&mut rm, &line_buf[..5], 0));
        assert_eq!(rm.match_start(), 2);
        assert_eq!(rm.match_end(), 5);
        assert!(!vim_regexec(&mut rm, &line_buf[..4], 0));
        rm.free_prog();
    }

    #[test]
    fn nfa_regprog_is_send_sync_and_matches_concurrently_across_threads() {
        let _guard = crate::globals::TEST_GLOBAL_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _editor = crate::main_init::test_editor();

        let prog = vim_regcomp(b"^\\s*fn\\s\\+\\(\\w\\+\\)_item_[0-9]\\+$", RE_MAGIC).unwrap();
        let b_chartab = curbuf().b_chartab.get();
        let (isf_chartab, isi_chartab) = crate::charset::get_global_chartabs();
        let p_mmp = opt_vars().p_mmp.get();

        with_pure_nfa_prog(&RegprogPtr::from_box(Some(prog)), |nfa| {
            let results: Vec<Vec<(usize, Colnr, Colnr)>> = std::thread::scope(|s| {
                let mut handles = Vec::new();
                for t in 0..4usize {
                    handles.push(s.spawn(move || {
                        init_worker_regex_env(isf_chartab, isi_chartab, p_mmp);
                        let mut matches = Vec::new();
                        for i in 0..200usize {
                            let line = if i % 10 == t {
                                format!("  fn worker_{t}_item_{i}")
                            } else {
                                format!("  let non_matching_line = {i};")
                            };
                            if let Ok(Some((sc, ec))) =
                                nfa_match_line_slice(nfa, line.as_bytes(), 0, false, 0, b_chartab)
                            {
                                matches.push((i, sc, ec));
                            }
                        }
                        matches
                    }));
                }
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });

            for (t, worker_matches) in results.into_iter().enumerate() {
                assert_eq!(worker_matches.len(), 20);
                for (k, (i, sc, ec)) in worker_matches.into_iter().enumerate() {
                    let expected_i = k * 10 + t;
                    let expected_len = format!("  fn worker_{t}_item_{expected_i}").len() as Colnr;
                    assert_eq!(i, expected_i);
                    assert_eq!(sc, 0);
                    assert_eq!(ec, expected_len);
                }
            }
        })
        .expect("expected pure single-line NFA program");
    }
}
