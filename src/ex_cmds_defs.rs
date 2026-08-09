//! Translated from `src/nvim/ex_cmds_defs.h` (partial).
//!
//! Translated: the `EX_*`/`BAD_*`/`FORCE_*`/`EXFLAG_*`/`CMOD_*`
//! constants, `cmd_addr_T`, `cmdmod_T`, `SubReplacementString`, and now
//! `cmdidx_T` (as [`CmdIdxT`]) + `exarg_T` (as [`ExargT`]) +
//! `CommandDefinition` + `ex_func_T`/`ex_preview_func_T`/`LineGetter`.
//!
//! `cmdidx_T` is mechanically transcribed from
//! `build/include/ex_cmds_enum.generated.h` (itself generated from
//! `src/nvim/ex_cmds.lua`'s master Ex-command table, per this project's
//! item 5 of "confirmed decisions": translate the generator's *output*,
//! not build a codegen pipeline) via a throwaway Python script, the
//! same technique already used for `option_defs.rs`'s `OptIndex`
//! family - verified via the script's own sanity assertions (561 real
//! commands matching `ex_cmds_defs.generated.h`'s `command_count = 561`,
//! exactly the 2 known negative sentinels `CMD_USER`/`CMD_USER_BUF`,
//! zero unexpected duplicate names) AND an independent, separately-
//! computed cross-check (line-number-derived index for 14 scattered
//! entries, including every Rust-keyword-colliding name) before
//! trusting the generated code. Deleted after use.
//!
//! Unlike `OptIndex` (whose option names are all already lowercase,
//! so a "capitalize first letter" PascalCase transform never
//! collides), `cmdidx_T`'s names preserve meaningful mixed case from
//! the real Vim command spelling (e.g. `next`/`Next` are two genuinely
//! DIFFERENT commands - `:next`/`ex_next` vs `:Next`/`ex_previous`,
//! confirmed by reading `ex_cmds_defs.generated.h` directly) - a
//! "capitalize first letter" transform was tried first and correctly
//! rejected once this exact collision was caught by the transcription
//! script's own duplicate-name assertion. Every variant instead keeps
//! its *exact* original spelling (`append`, `bNext`, `next`, `Next`,
//! ...), matching `globals.rs`'s own established "preserve exact
//! original spelling, `#[allow]` the resulting lint" precedent
//! (`#[allow(non_camel_case_types)]` here) rather than force-renaming
//! to fit Rust conventions. 12 names are Rust reserved keywords
//! (`break`, `const`, `continue`, `else`, `for`, `if`, `let`, `match`,
//! `move`, `return`, `try`, `while`) and use raw-identifier syntax
//! (`r#break`, etc.) to stay valid.
//!
//! `exarg_T` (as [`ExargT`]) is translated field-for-field: `char*`
//! fields become `Option<Vec<u8>>` (this crate's usual convention);
//! `args`/`arglens`/`argc` (an allocated `char**` + parallel
//! `size_t*` + count) collapse into one `Vec<Vec<u8>>` (each element
//! already knows its own length, matching the `tv_dict_add_str`-style
//! "no separate len variant" precedent); `cstack: *mut CstackT` uses
//! the already-translated `crate::ex_eval_defs::CstackT`; the
//! `struct { bool file; bool bar; } magic` anonymous struct becomes
//! two plain fields (`magic_file`/`magic_bar`); `cmdlinep: char**`
//! (a pointer-to-the-caller's-own-pointer, letting a command handler
//! reallocate/replace the shared command-line buffer) is kept as a
//! raw `*mut *mut std::os::raw::c_char` for now - genuinely C-specific
//! indirection with no real caller yet to motivate a better shape.
//!
//! `CommandDefinition`'s actual *populated* 561-entry table
//! (`cmdnames[]`, `ex_cmds_defs.generated.h`) is deliberately NOT
//! translated here - every entry's `cmd_func`/`cmd_preview_func` would
//! need a real, already-translated Rust function for all 561 `ex_*`
//! command handlers (none exist), a separate, much larger future
//! undertaking (mirroring how `option_defs.rs`'s `OptIndex` enum was
//! translated well before the real `options[]` table + its
//! `opt_did_set_cb` callbacks). `ex_func_T`/`ex_preview_func_T`/
//! `LineGetter` are translated as plain Rust function-pointer type
//! aliases matching the original's C signatures exactly (not yet
//! idiomatic-Rust-ified, since nothing constructs a real one yet).
//!
//! `cmdmod_T.cmod_filter_regmatch` uses the new
//! `crate::types_defs::RegmatchT` opaque placeholder (`regexp_defs.h`,
//! phase 7) rather than waiting for the full regexp engine.

use crate::ex_eval_defs::CstackT;
use crate::os::time_defs::Timestamp;
use crate::pos_defs::LinenrT;
use crate::types_defs::{AdditionalData, HandleT, OptInt, RegmatchT};

/// Flags for `CommandDefinition`/`exarg_T.argt` (kept as plain `u32`
/// bit-flag constants, same reasoning as `HL_*`/`MarkMoveRes` elsewhere
/// in this crate).
pub mod ex_flags {
    /// allow a linespecs
    pub const RANGE: u32 = 0x001;
    /// allow a ! after the command name
    pub const BANG: u32 = 0x002;
    /// allow extra args after command name
    pub const EXTRA: u32 = 0x004;
    /// expand wildcards in extra part
    pub const XFILE: u32 = 0x008;
    /// extra part is a single argument (no split on whitespace)
    pub const NOSPC: u32 = 0x010;
    /// default file range is 1,$
    pub const DFLALL: u32 = 0x020;
    /// extend range to include whole fold also when less than two
    /// numbers given
    pub const WHOLEFOLD: u32 = 0x040;
    /// argument required
    pub const NEEDARG: u32 = 0x080;
    /// check for trailing vertical bar
    pub const TRLBAR: u32 = 0x100;
    /// allow "x for register designation
    pub const REGSTR: u32 = 0x200;
    /// allow count in argument, after command
    pub const COUNT: u32 = 0x400;
    /// no trailing comment allowed
    pub const NOTRLCOM: u32 = 0x800;
    /// zero line number allowed
    pub const ZEROR: u32 = 0x1000;
    /// do not remove CTRL-V from argument
    pub const CTRLV: u32 = 0x2000;
    /// allow "+command" argument
    pub const CMDARG: u32 = 0x4000;
    /// accepts buffer name
    pub const BUFNAME: u32 = 0x8000;
    /// accepts unlisted buffer too
    pub const BUFUNL: u32 = 0x10000;
    /// allow "++opt=val" argument
    pub const ARGOPT: u32 = 0x20000;
    /// allowed in the sandbox
    pub const SBOXOK: u32 = 0x40000;
    /// Command is allowed when curbuf is `b_ro_locked` (e.g. during a
    /// quickfix or diff critical section). Legacy name: `EX_CMDWIN`.
    /// Implies [`LOCK_OK`].
    pub const BUFLOCK_OK: u32 = 0x80000;
    /// forbidden in non-`'modifiable'` buffer
    pub const MODIFY: u32 = 0x100000;
    /// allow flags after count in argument
    pub const FLAGS: u32 = 0x200000;
    /// Command allowed when `textlock` is set. `BUFLOCK_OK` is
    /// per-buffer.
    pub const LOCK_OK: u32 = 0x1000000;
    /// keep sctx of where command was invoked
    pub const KEEPSCRIPT: u32 = 0x4000000;
    /// allow incremental command preview
    pub const PREVIEW: u32 = 0x8000000;
    /// completion: keep spaces in arg lead
    pub const ARGSPACE: u32 = 0x40000000;
    /// multiple extra files allowed
    pub const FILES: u32 = XFILE | EXTRA;
    /// 1 file, defaults to current file
    pub const FILE1: u32 = FILES | NOSPC;
    /// one extra word allowed
    pub const WORD1: u32 = EXTRA | NOSPC;
}

/// Values for `cmd_addr_type` (`cmd_addr_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CmdAddrT {
    /// buffer line numbers
    #[default]
    Lines,
    /// window number
    Windows,
    /// argument number
    Arguments,
    /// buffer number of loaded buffer
    LoadedBuffers,
    /// buffer number
    Buffers,
    /// tab page number
    Tabs,
    /// Tab page that only relative
    TabsRelative,
    /// quickfix list valid entry number
    QuickfixValid,
    /// quickfix list entry number
    Quickfix,
    /// positive count or zero, defaults to 1
    Unsigned,
    /// something else, use line number for '$', '%', etc.
    Other,
    /// no range used
    None,
}

// Behavior for bad character, "++bad=" argument.
/// replace it with '?' (default) (`BAD_REPLACE`).
pub const BAD_REPLACE: u8 = b'?';
/// leave it (`BAD_KEEP`).
pub const BAD_KEEP: i32 = -1;
/// erase it (`BAD_DROP`).
pub const BAD_DROP: i32 = -2;

/// `":edit ++bin file"` (`FORCE_BIN`).
pub const FORCE_BIN: i32 = 1;
/// `":edit ++nobin file"` (`FORCE_NOBIN`).
pub const FORCE_NOBIN: i32 = 2;

/// Values for `exarg_T.flags` (`EXFLAG_*`).
pub mod exflag {
    /// 'l': list
    pub const LIST: i32 = 0x01;
    /// '#': number
    pub const NR: i32 = 0x02;
    /// 'p': print
    pub const PRINT: i32 = 0x04;
}

/// Command modifier flags for [`CmdmodT.cmod_flags`](CmdmodT::cmod_flags)
/// (`CMOD_*`).
pub mod cmod {
    /// `":sandbox"`
    pub const SANDBOX: i32 = 0x0001;
    /// `":silent"`
    pub const SILENT: i32 = 0x0002;
    /// `":silent!"`
    pub const ERRSILENT: i32 = 0x0004;
    /// `":unsilent"`
    pub const UNSILENT: i32 = 0x0008;
    /// `":noautocmd"`
    pub const NOAUTOCMD: i32 = 0x0010;
    /// `":hide"`
    pub const HIDE: i32 = 0x0020;
    /// `":browse"` - invoke file dialog
    pub const BROWSE: i32 = 0x0040;
    /// `":confirm"` - invoke yes/no dialog
    pub const CONFIRM: i32 = 0x0080;
    /// `":keepalt"`
    pub const KEEPALT: i32 = 0x0100;
    /// `":keepmarks"`
    pub const KEEPMARKS: i32 = 0x0200;
    /// `":keepjumps"`
    pub const KEEPJUMPS: i32 = 0x0400;
    /// `":lockmarks"`
    pub const LOCKMARKS: i32 = 0x0800;
    /// `":keeppatterns"`
    pub const KEEPPATTERNS: i32 = 0x1000;
    /// `":noswapfile"`
    pub const NOSWAPFILE: i32 = 0x2000;
}

/// Command modifiers `":vertical"`, `":browse"`, `":confirm"`, `":hide"`,
/// etc. set a flag. This needs to be saved for recursive commands, put
/// them in a structure for easy manipulation (`cmdmod_T`).
#[derive(Debug, Clone, Default)]
pub struct CmdmodT {
    /// `CMOD_*` flags
    pub cmod_flags: i32,

    /// flags for `win_split()`
    pub cmod_split: i32,
    /// > 0 when `":tab"` was used
    pub cmod_tab: i32,
    pub cmod_filter_pat: Option<Vec<u8>>,
    /// set by `:filter /pat/`
    pub cmod_filter_regmatch: RegmatchT,
    /// set for `:filter!`
    pub cmod_filter_force: bool,

    /// 0 if not set, > 0 to set `'verbose'` to `cmod_verbose - 1`
    pub cmod_verbose: i32,

    // Values for undo_cmdmod() (not yet translated).
    /// saved value of `'eventignore'`
    pub cmod_save_ei: Option<Vec<u8>>,
    /// set when "sandbox" was incremented
    pub cmod_did_sandbox: i32,
    /// if `'verbose'` was set: value of `p_verbose` plus one
    pub cmod_verbose_save: OptInt,
    /// if non-zero: saved value of `msg_silent + 1`
    pub cmod_save_msg_silent: i32,
    /// for restoring `msg_scroll`
    pub cmod_save_msg_scroll: i32,
    /// incremented when `emsg_silent` is (comment is truncated
    /// mid-sentence in the original C source too - not a translation
    /// error here).
    pub cmod_did_esilent: i32,
}

/// Previous `:substitute` replacement string definition
/// (`SubReplacementString`).
#[derive(Debug, Clone, Default)]
pub struct SubReplacementString {
    /// Previous replacement string.
    pub sub: Option<Vec<u8>>,
    /// Time when it was last set.
    pub timestamp: Timestamp,
    /// Additional data left from ShaDa file.
    pub additional_data: Option<Box<AdditionalData>>,
}

/// The index for an Ex command (`cmdidx_T`) - see this module's own
/// doc comment for the full transcription/verification methodology
/// and the exact-casing-preservation rationale.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum CmdIdxT {
    USER = -1,
    USER_BUF = -2,
    append = 0,
    abbreviate = 1,
    abclear = 2,
    aboveleft = 3,
    all = 4,
    amenu = 5,
    anoremenu = 6,
    args = 7,
    argadd = 8,
    argdelete = 9,
    argdo = 10,
    argdedupe = 11,
    argedit = 12,
    argglobal = 13,
    arglocal = 14,
    argument = 15,
    ascii = 16,
    autocmd = 17,
    augroup = 18,
    aunmenu = 19,
    buffer = 20,
    bNext = 21,
    ball = 22,
    badd = 23,
    balt = 24,
    bdelete = 25,
    belowright = 26,
    bfirst = 27,
    blast = 28,
    bmodified = 29,
    bnext = 30,
    botright = 31,
    bprevious = 32,
    brewind = 33,
    r#break = 34,
    breakadd = 35,
    breakdel = 36,
    breaklist = 37,
    browse = 38,
    buffers = 39,
    bufdo = 40,
    bunload = 41,
    bwipeout = 42,
    change = 43,
    cNext = 44,
    cNfile = 45,
    cabbrev = 46,
    cabclear = 47,
    cabove = 48,
    caddbuffer = 49,
    caddexpr = 50,
    caddfile = 51,
    cafter = 52,
    call = 53,
    catch = 54,
    cbuffer = 55,
    cbefore = 56,
    cbelow = 57,
    cbottom = 58,
    cc = 59,
    cclose = 60,
    cd = 61,
    cdo = 62,
    center = 63,
    cexpr = 64,
    cfile = 65,
    cfdo = 66,
    cfirst = 67,
    cgetfile = 68,
    cgetbuffer = 69,
    cgetexpr = 70,
    chdir = 71,
    changes = 72,
    checkhealth = 73,
    checkpath = 74,
    checktime = 75,
    chistory = 76,
    clist = 77,
    clast = 78,
    close = 79,
    clearjumps = 80,
    cmap = 81,
    cmapclear = 82,
    cmenu = 83,
    cnext = 84,
    cnewer = 85,
    cnfile = 86,
    cnoremap = 87,
    cnoreabbrev = 88,
    cnoremenu = 89,
    copy = 90,
    colder = 91,
    colorscheme = 92,
    command = 93,
    comclear = 94,
    compiler = 95,
    r#continue = 96,
    confirm = 97,
    connect = 98,
    r#const = 99,
    copen = 100,
    cprevious = 101,
    cpfile = 102,
    cquit = 103,
    crewind = 104,
    cunmap = 105,
    cunabbrev = 106,
    cunmenu = 107,
    cwindow = 108,
    delete = 109,
    delmarks = 110,
    debug = 111,
    debuggreedy = 112,
    defer = 113,
    delcommand = 114,
    delfunction = 115,
    detach = 116,
    display = 117,
    diffupdate = 118,
    diffget = 119,
    diffoff = 120,
    diffpatch = 121,
    diffput = 122,
    diffsplit = 123,
    diffthis = 124,
    digraphs = 125,
    djump = 126,
    dlist = 127,
    doautocmd = 128,
    doautoall = 129,
    drop = 130,
    dsearch = 131,
    dsplit = 132,
    edit = 133,
    earlier = 134,
    echo = 135,
    echoerr = 136,
    echohl = 137,
    echomsg = 138,
    echon = 139,
    r#else = 140,
    elseif = 141,
    emenu = 142,
    endif = 143,
    endfunction = 144,
    endfor = 145,
    endtry = 146,
    endwhile = 147,
    enew = 148,
    eval = 149,
    ex = 150,
    execute = 151,
    exit = 152,
    exusage = 153,
    file = 154,
    files = 155,
    filetype = 156,
    filter = 157,
    find = 158,
    finally = 159,
    finish = 160,
    first = 161,
    fold = 162,
    foldclose = 163,
    folddoopen = 164,
    folddoclosed = 165,
    foldopen = 166,
    r#for = 167,
    function = 168,
    fclose = 169,
    global = 170,
    goto = 171,
    grep = 172,
    grepadd = 173,
    gui = 174,
    gvim = 175,
    help = 176,
    helpclose = 177,
    helpgrep = 178,
    helptags = 179,
    highlight = 180,
    hide = 181,
    history = 182,
    horizontal = 183,
    insert = 184,
    iabbrev = 185,
    iabclear = 186,
    r#if = 187,
    ijump = 188,
    ilist = 189,
    imap = 190,
    imapclear = 191,
    imenu = 192,
    inoremap = 193,
    inoreabbrev = 194,
    inoremenu = 195,
    intro = 196,
    iput = 197,
    isearch = 198,
    isplit = 199,
    iunmap = 200,
    iunabbrev = 201,
    iunmenu = 202,
    join = 203,
    jumps = 204,
    k = 205,
    keepmarks = 206,
    keepjumps = 207,
    keeppatterns = 208,
    keepalt = 209,
    list = 210,
    lNext = 211,
    lNfile = 212,
    last = 213,
    labove = 214,
    language = 215,
    laddexpr = 216,
    laddbuffer = 217,
    laddfile = 218,
    lafter = 219,
    later = 220,
    lbuffer = 221,
    lbefore = 222,
    lbelow = 223,
    lbottom = 224,
    lcd = 225,
    lchdir = 226,
    lclose = 227,
    ldo = 228,
    left = 229,
    leftabove = 230,
    r#let = 231,
    lexpr = 232,
    lfile = 233,
    lfdo = 234,
    lfirst = 235,
    lgetfile = 236,
    lgetbuffer = 237,
    lgetexpr = 238,
    lgrep = 239,
    lgrepadd = 240,
    lhelpgrep = 241,
    lhistory = 242,
    ll = 243,
    llast = 244,
    llist = 245,
    lmap = 246,
    lmapclear = 247,
    lmake = 248,
    lnoremap = 249,
    lnext = 250,
    lnewer = 251,
    lnfile = 252,
    loadview = 253,
    loadkeymap = 254,
    lockmarks = 255,
    lockvar = 256,
    log = 257,
    lolder = 258,
    lopen = 259,
    lprevious = 260,
    lpfile = 261,
    lrewind = 262,
    ltag = 263,
    lunmap = 264,
    lua = 265,
    luado = 266,
    luafile = 267,
    lvimgrep = 268,
    lvimgrepadd = 269,
    lwindow = 270,
    ls = 271,
    lsp = 272,
    r#move = 273,
    mark = 274,
    make = 275,
    map = 276,
    mapclear = 277,
    marks = 278,
    r#match = 279,
    menu = 280,
    menutranslate = 281,
    messages = 282,
    mkexrc = 283,
    mksession = 284,
    mkspell = 285,
    mkvimrc = 286,
    mkview = 287,
    mode = 288,
    mzscheme = 289,
    mzfile = 290,
    next = 291,
    new = 292,
    nmap = 293,
    nmapclear = 294,
    nmenu = 295,
    nnoremap = 296,
    nnoremenu = 297,
    noremap = 298,
    noautocmd = 299,
    nohlsearch = 300,
    noreabbrev = 301,
    noremenu = 302,
    noswapfile = 303,
    normal = 304,
    number = 305,
    nunmap = 306,
    nunmenu = 307,
    oldfiles = 308,
    omap = 309,
    omapclear = 310,
    omenu = 311,
    only = 312,
    onoremap = 313,
    onoremenu = 314,
    options = 315,
    ounmap = 316,
    ounmenu = 317,
    ownsyntax = 318,
    print = 319,
    packadd = 320,
    packdel = 321,
    packloadall = 322,
    packupdate = 323,
    pbuffer = 324,
    pclose = 325,
    perl = 326,
    perldo = 327,
    perlfile = 328,
    pedit = 329,
    pop = 330,
    popup = 331,
    ppop = 332,
    preserve = 333,
    previous = 334,
    profile = 335,
    profdel = 336,
    psearch = 337,
    ptag = 338,
    ptNext = 339,
    ptfirst = 340,
    ptjump = 341,
    ptlast = 342,
    ptnext = 343,
    ptprevious = 344,
    ptrewind = 345,
    ptselect = 346,
    put = 347,
    pwd = 348,
    python = 349,
    pydo = 350,
    pyfile = 351,
    py3 = 352,
    py3do = 353,
    python3 = 354,
    py3file = 355,
    pyx = 356,
    pyxdo = 357,
    pythonx = 358,
    pyxfile = 359,
    quit = 360,
    quitall = 361,
    qall = 362,
    read = 363,
    recover = 364,
    redo = 365,
    redir = 366,
    redraw = 367,
    redrawstatus = 368,
    redrawtabline = 369,
    registers = 370,
    resize = 371,
    restart = 372,
    retab = 373,
    r#return = 374,
    rewind = 375,
    right = 376,
    rightbelow = 377,
    rshada = 378,
    runtime = 379,
    rundo = 380,
    ruby = 381,
    rubydo = 382,
    rubyfile = 383,
    rviminfo = 384,
    substitute = 385,
    sNext = 386,
    sargument = 387,
    sall = 388,
    sandbox = 389,
    saveas = 390,
    sbuffer = 391,
    sbNext = 392,
    sball = 393,
    sbfirst = 394,
    sblast = 395,
    sbmodified = 396,
    sbnext = 397,
    sbprevious = 398,
    sbrewind = 399,
    scriptnames = 400,
    scriptencoding = 401,
    set = 402,
    setfiletype = 403,
    setglobal = 404,
    setlocal = 405,
    sfind = 406,
    sfirst = 407,
    simalt = 408,
    sign = 409,
    silent = 410,
    sleep = 411,
    slast = 412,
    smagic = 413,
    smap = 414,
    smapclear = 415,
    smenu = 416,
    snext = 417,
    snomagic = 418,
    snoremap = 419,
    snoremenu = 420,
    source = 421,
    sort = 422,
    split = 423,
    spellgood = 424,
    spelldump = 425,
    spellinfo = 426,
    spellrepall = 427,
    spellrare = 428,
    spellundo = 429,
    spellwrong = 430,
    sprevious = 431,
    srewind = 432,
    stop = 433,
    stag = 434,
    startinsert = 435,
    startgreplace = 436,
    startreplace = 437,
    stopinsert = 438,
    stjump = 439,
    stselect = 440,
    sunhide = 441,
    sunmap = 442,
    sunmenu = 443,
    suspend = 444,
    sview = 445,
    swapname = 446,
    syntax = 447,
    syntime = 448,
    syncbind = 449,
    t = 450,
    tcd = 451,
    tchdir = 452,
    tNext = 453,
    tag = 454,
    tags = 455,
    tab = 456,
    tabclose = 457,
    tabdo = 458,
    tabedit = 459,
    tabfind = 460,
    tabfirst = 461,
    tabmove = 462,
    tablast = 463,
    tabnext = 464,
    tabnew = 465,
    tabonly = 466,
    tabprevious = 467,
    tabNext = 468,
    tabrewind = 469,
    tabs = 470,
    tcl = 471,
    tcldo = 472,
    tclfile = 473,
    terminal = 474,
    tfirst = 475,
    throw = 476,
    tjump = 477,
    tlast = 478,
    tlmenu = 479,
    tlnoremenu = 480,
    tlunmenu = 481,
    tmenu = 482,
    tmap = 483,
    tmapclear = 484,
    tnext = 485,
    tnoremap = 486,
    topleft = 487,
    tprevious = 488,
    trewind = 489,
    trust = 490,
    r#try = 491,
    tselect = 492,
    tunmenu = 493,
    tunmap = 494,
    undo = 495,
    undojoin = 496,
    undolist = 497,
    unabbreviate = 498,
    unhide = 499,
    uniq = 500,
    unlet = 501,
    unlockvar = 502,
    unmap = 503,
    unmenu = 504,
    unsilent = 505,
    update = 506,
    uptime = 507,
    vglobal = 508,
    version = 509,
    verbose = 510,
    vertical = 511,
    visual = 512,
    view = 513,
    vimgrep = 514,
    vimgrepadd = 515,
    viusage = 516,
    vmap = 517,
    vmapclear = 518,
    vmenu = 519,
    vnoremap = 520,
    vnew = 521,
    vnoremenu = 522,
    vsplit = 523,
    vunmap = 524,
    vunmenu = 525,
    write = 526,
    wNext = 527,
    wall = 528,
    r#while = 529,
    winsize = 530,
    wincmd = 531,
    windo = 532,
    winpos = 533,
    wnext = 534,
    wprevious = 535,
    wq = 536,
    wqall = 537,
    wshada = 538,
    wundo = 539,
    wviminfo = 540,
    xit = 541,
    xall = 542,
    xmap = 543,
    xmapclear = 544,
    xmenu = 545,
    xnoremap = 546,
    xnoremenu = 547,
    xunmap = 548,
    xunmenu = 549,
    yank = 550,
    z = 551,
    bang = 552,
    pound = 553,
    and = 554,
    lshift = 555,
    equal = 556,
    rshift = 557,
    at = 558,
    tilde = 559,
    Next = 560,
    SIZE = 561,
}

/// The name of every Ex command, indexed by its [`CmdIdxT`]
/// discriminant (`cmdnames[].cmd_name`).
///
/// The original's `cmdnames[]` entries also carry `cmd_argt`,
/// `cmd_addr_type`, `cmd_func` and `cmd_preview_func`. Only the NAMES
/// are translated here, because the function-pointer fields would each
/// need a real translated `ex_*` handler and none exist yet (see this
/// module's own doc comment). Name-only lookups - `is_loclist_cmd`,
/// command-name matching, completion - need nothing more than this.
///
/// Mechanically transcribed from `src/nvim/ex_cmds.lua` (the single
/// source the generated `ex_cmds_defs.generated.h` is built from) by a
/// throwaway script, matching how `OptIndex` and [`CmdIdxT`] itself
/// were produced. The script verified two things independently before
/// emitting anything: that the table holds exactly
/// [`CmdIdxT::SIZE`] entries, and that EVERY entry's name matches the
/// `CmdIdxT` variant carrying that same discriminant - accounting for
/// the Rust keywords spelled as raw identifiers (`r#break`) and the
/// eight punctuation commands whose variants are spelled out
/// (`bang`, `pound`, `and`, `lshift`, `equal`, `rshift`, `at`,
/// `tilde`). Zero mismatches across all 561 entries.
pub static CMDNAMES: [&[u8]; CmdIdxT::SIZE as usize] = [
    b"append", b"abbreviate", b"abclear", b"aboveleft",
    b"all", b"amenu", b"anoremenu", b"args",
    b"argadd", b"argdelete", b"argdo", b"argdedupe",
    b"argedit", b"argglobal", b"arglocal", b"argument",
    b"ascii", b"autocmd", b"augroup", b"aunmenu",
    b"buffer", b"bNext", b"ball", b"badd",
    b"balt", b"bdelete", b"belowright", b"bfirst",
    b"blast", b"bmodified", b"bnext", b"botright",
    b"bprevious", b"brewind", b"break", b"breakadd",
    b"breakdel", b"breaklist", b"browse", b"buffers",
    b"bufdo", b"bunload", b"bwipeout", b"change",
    b"cNext", b"cNfile", b"cabbrev", b"cabclear",
    b"cabove", b"caddbuffer", b"caddexpr", b"caddfile",
    b"cafter", b"call", b"catch", b"cbuffer",
    b"cbefore", b"cbelow", b"cbottom", b"cc",
    b"cclose", b"cd", b"cdo", b"center",
    b"cexpr", b"cfile", b"cfdo", b"cfirst",
    b"cgetfile", b"cgetbuffer", b"cgetexpr", b"chdir",
    b"changes", b"checkhealth", b"checkpath", b"checktime",
    b"chistory", b"clist", b"clast", b"close",
    b"clearjumps", b"cmap", b"cmapclear", b"cmenu",
    b"cnext", b"cnewer", b"cnfile", b"cnoremap",
    b"cnoreabbrev", b"cnoremenu", b"copy", b"colder",
    b"colorscheme", b"command", b"comclear", b"compiler",
    b"continue", b"confirm", b"connect", b"const",
    b"copen", b"cprevious", b"cpfile", b"cquit",
    b"crewind", b"cunmap", b"cunabbrev", b"cunmenu",
    b"cwindow", b"delete", b"delmarks", b"debug",
    b"debuggreedy", b"defer", b"delcommand", b"delfunction",
    b"detach", b"display", b"diffupdate", b"diffget",
    b"diffoff", b"diffpatch", b"diffput", b"diffsplit",
    b"diffthis", b"digraphs", b"djump", b"dlist",
    b"doautocmd", b"doautoall", b"drop", b"dsearch",
    b"dsplit", b"edit", b"earlier", b"echo",
    b"echoerr", b"echohl", b"echomsg", b"echon",
    b"else", b"elseif", b"emenu", b"endif",
    b"endfunction", b"endfor", b"endtry", b"endwhile",
    b"enew", b"eval", b"ex", b"execute",
    b"exit", b"exusage", b"file", b"files",
    b"filetype", b"filter", b"find", b"finally",
    b"finish", b"first", b"fold", b"foldclose",
    b"folddoopen", b"folddoclosed", b"foldopen", b"for",
    b"function", b"fclose", b"global", b"goto",
    b"grep", b"grepadd", b"gui", b"gvim",
    b"help", b"helpclose", b"helpgrep", b"helptags",
    b"highlight", b"hide", b"history", b"horizontal",
    b"insert", b"iabbrev", b"iabclear", b"if",
    b"ijump", b"ilist", b"imap", b"imapclear",
    b"imenu", b"inoremap", b"inoreabbrev", b"inoremenu",
    b"intro", b"iput", b"isearch", b"isplit",
    b"iunmap", b"iunabbrev", b"iunmenu", b"join",
    b"jumps", b"k", b"keepmarks", b"keepjumps",
    b"keeppatterns", b"keepalt", b"list", b"lNext",
    b"lNfile", b"last", b"labove", b"language",
    b"laddexpr", b"laddbuffer", b"laddfile", b"lafter",
    b"later", b"lbuffer", b"lbefore", b"lbelow",
    b"lbottom", b"lcd", b"lchdir", b"lclose",
    b"ldo", b"left", b"leftabove", b"let",
    b"lexpr", b"lfile", b"lfdo", b"lfirst",
    b"lgetfile", b"lgetbuffer", b"lgetexpr", b"lgrep",
    b"lgrepadd", b"lhelpgrep", b"lhistory", b"ll",
    b"llast", b"llist", b"lmap", b"lmapclear",
    b"lmake", b"lnoremap", b"lnext", b"lnewer",
    b"lnfile", b"loadview", b"loadkeymap", b"lockmarks",
    b"lockvar", b"log", b"lolder", b"lopen",
    b"lprevious", b"lpfile", b"lrewind", b"ltag",
    b"lunmap", b"lua", b"luado", b"luafile",
    b"lvimgrep", b"lvimgrepadd", b"lwindow", b"ls",
    b"lsp", b"move", b"mark", b"make",
    b"map", b"mapclear", b"marks", b"match",
    b"menu", b"menutranslate", b"messages", b"mkexrc",
    b"mksession", b"mkspell", b"mkvimrc", b"mkview",
    b"mode", b"mzscheme", b"mzfile", b"next",
    b"new", b"nmap", b"nmapclear", b"nmenu",
    b"nnoremap", b"nnoremenu", b"noremap", b"noautocmd",
    b"nohlsearch", b"noreabbrev", b"noremenu", b"noswapfile",
    b"normal", b"number", b"nunmap", b"nunmenu",
    b"oldfiles", b"omap", b"omapclear", b"omenu",
    b"only", b"onoremap", b"onoremenu", b"options",
    b"ounmap", b"ounmenu", b"ownsyntax", b"print",
    b"packadd", b"packdel", b"packloadall", b"packupdate",
    b"pbuffer", b"pclose", b"perl", b"perldo",
    b"perlfile", b"pedit", b"pop", b"popup",
    b"ppop", b"preserve", b"previous", b"profile",
    b"profdel", b"psearch", b"ptag", b"ptNext",
    b"ptfirst", b"ptjump", b"ptlast", b"ptnext",
    b"ptprevious", b"ptrewind", b"ptselect", b"put",
    b"pwd", b"python", b"pydo", b"pyfile",
    b"py3", b"py3do", b"python3", b"py3file",
    b"pyx", b"pyxdo", b"pythonx", b"pyxfile",
    b"quit", b"quitall", b"qall", b"read",
    b"recover", b"redo", b"redir", b"redraw",
    b"redrawstatus", b"redrawtabline", b"registers", b"resize",
    b"restart", b"retab", b"return", b"rewind",
    b"right", b"rightbelow", b"rshada", b"runtime",
    b"rundo", b"ruby", b"rubydo", b"rubyfile",
    b"rviminfo", b"substitute", b"sNext", b"sargument",
    b"sall", b"sandbox", b"saveas", b"sbuffer",
    b"sbNext", b"sball", b"sbfirst", b"sblast",
    b"sbmodified", b"sbnext", b"sbprevious", b"sbrewind",
    b"scriptnames", b"scriptencoding", b"set", b"setfiletype",
    b"setglobal", b"setlocal", b"sfind", b"sfirst",
    b"simalt", b"sign", b"silent", b"sleep",
    b"slast", b"smagic", b"smap", b"smapclear",
    b"smenu", b"snext", b"snomagic", b"snoremap",
    b"snoremenu", b"source", b"sort", b"split",
    b"spellgood", b"spelldump", b"spellinfo", b"spellrepall",
    b"spellrare", b"spellundo", b"spellwrong", b"sprevious",
    b"srewind", b"stop", b"stag", b"startinsert",
    b"startgreplace", b"startreplace", b"stopinsert", b"stjump",
    b"stselect", b"sunhide", b"sunmap", b"sunmenu",
    b"suspend", b"sview", b"swapname", b"syntax",
    b"syntime", b"syncbind", b"t", b"tcd",
    b"tchdir", b"tNext", b"tag", b"tags",
    b"tab", b"tabclose", b"tabdo", b"tabedit",
    b"tabfind", b"tabfirst", b"tabmove", b"tablast",
    b"tabnext", b"tabnew", b"tabonly", b"tabprevious",
    b"tabNext", b"tabrewind", b"tabs", b"tcl",
    b"tcldo", b"tclfile", b"terminal", b"tfirst",
    b"throw", b"tjump", b"tlast", b"tlmenu",
    b"tlnoremenu", b"tlunmenu", b"tmenu", b"tmap",
    b"tmapclear", b"tnext", b"tnoremap", b"topleft",
    b"tprevious", b"trewind", b"trust", b"try",
    b"tselect", b"tunmenu", b"tunmap", b"undo",
    b"undojoin", b"undolist", b"unabbreviate", b"unhide",
    b"uniq", b"unlet", b"unlockvar", b"unmap",
    b"unmenu", b"unsilent", b"update", b"uptime",
    b"vglobal", b"version", b"verbose", b"vertical",
    b"visual", b"view", b"vimgrep", b"vimgrepadd",
    b"viusage", b"vmap", b"vmapclear", b"vmenu",
    b"vnoremap", b"vnew", b"vnoremenu", b"vsplit",
    b"vunmap", b"vunmenu", b"write", b"wNext",
    b"wall", b"while", b"winsize", b"wincmd",
    b"windo", b"winpos", b"wnext", b"wprevious",
    b"wq", b"wqall", b"wshada", b"wundo",
    b"wviminfo", b"xit", b"xall", b"xmap",
    b"xmapclear", b"xmenu", b"xnoremap", b"xnoremenu",
    b"xunmap", b"xunmenu", b"yank", b"z",
    b"!", b"#", b"&", b"<",
    b"=", b">", b"@", b"~",
    b"Next",
];

/// The name of the Ex command `cmdidx`, or `None` for a user command
/// or an out-of-range index (`cmdnames[cmdidx].cmd_name`).
#[must_use]
pub fn cmdname(cmdidx: CmdIdxT) -> Option<&'static [u8]> {
    let idx = cmdidx as i32;
    if idx < 0 || idx >= CmdIdxT::SIZE as i32 {
        return None;
    }
    Some(CMDNAMES[idx as usize])
}

/// Function pointer type for a command's implementation (`ex_func_T`).
/// No real function currently populates a [`CommandDefinition`] with
/// one of these yet (every `ex_*` command handler across the whole
/// codebase is still deferred) - kept as a plain Rust function-pointer
/// type matching the original's C signature exactly, not yet
/// idiomatic-Rust-ified since there's no real usage to design around.
pub type ExFuncT = fn(eap: *mut ExargT);

/// Function pointer type for a command's incremental-preview
/// implementation (`ex_preview_func_T`).
pub type ExPreviewFuncT = fn(eap: *mut ExargT, cmdpreview_ns: i32, cmdpreview_bufnr: HandleT) -> i32;

/// Function pointer type used to fetch the next line of a multi-line
/// command (e.g. `:function`'s body) (`LineGetter`). Returns a raw,
/// nullable C string, matching the original exactly - not yet
/// idiomatic-Rust-ified since nothing calls through one yet.
pub type LineGetter = fn(
    c: i32,
    cookie: *mut std::ffi::c_void,
    indent: i32,
    do_concat: bool,
) -> *mut std::os::raw::c_char;

/// Structure for a command definition (`CommandDefinition`). The real,
/// populated 561-entry `cmdnames[]` table is deliberately not
/// translated here - see this module's own doc comment.
#[derive(Debug, Clone, Copy)]
pub struct CommandDefinition {
    /// Name of the command (`cmd_name`).
    pub cmd_name: &'static [u8],
    /// Function implementing this command; `None` matches the
    /// original's nullable `cmd_func` (`cmd_func`).
    pub cmd_func: Option<ExFuncT>,
    /// Preview callback function of this command; `None` matches the
    /// original's nullable `cmd_preview_func` (`cmd_preview_func`).
    pub cmd_preview_func: Option<ExPreviewFuncT>,
    /// Relevant flags from [`ex_flags`] (`cmd_argt`).
    pub cmd_argt: u32,
    /// Address type (`cmd_addr_type`).
    pub cmd_addr_type: CmdAddrT,
}

/// Arguments used for Ex commands (`exarg_T`).
///
/// See this module's own doc comment for the field-by-field
/// translation notes (the `args`/`arglens`/`argc` collapse, the
/// `magic` anonymous-struct split, `cmdlinep`'s raw-pointer-to-pointer
/// treatment, etc.).
#[derive(Debug)]
pub struct ExargT {
    /// Argument of the command (`arg`).
    pub arg: Option<Vec<u8>>,
    /// Command arguments, each already knowing its own length (`args`
    /// + `arglens` + `argc` collapsed into one `Vec`).
    pub args: Vec<Vec<u8>>,
    /// Next command, `None` if none (`nextcmd`).
    pub nextcmd: Option<Vec<u8>>,
    /// The name of the command (except for `:make`) (`cmd`).
    pub cmd: Option<Vec<u8>>,
    /// Pointer to pointer of allocated cmdline (`cmdlinep`) - kept as
    /// a raw pointer-to-pointer, see this module's own doc comment.
    pub cmdlinep: *mut *mut std::os::raw::c_char,
    /// Free later (`cmdline_tofree`).
    pub cmdline_tofree: Option<Vec<u8>>,
    /// The index for the command (`cmdidx`).
    pub cmdidx: CmdIdxT,
    /// Flags for the command, from [`ex_flags`] (`argt`).
    pub argt: u32,
    /// Don't execute the command, only parse it (`skip`).
    pub skip: bool,
    /// `true` if `!` present (`forceit`).
    pub forceit: bool,
    /// The number of addresses given (`addr_count`).
    pub addr_count: i32,
    /// The first line number (`line1`).
    pub line1: LinenrT,
    /// The second line number or count (`line2`).
    pub line2: LinenrT,
    /// Type of the count/range (`addr_type`).
    pub addr_type: CmdAddrT,
    /// Extra flags after count, from [`exflag`] (`flags`).
    pub flags: i32,
    /// `+command` arg to be used in edited file (`do_ecmd_cmd`).
    pub do_ecmd_cmd: Option<Vec<u8>>,
    /// The line number in an edited file (`do_ecmd_lnum`).
    pub do_ecmd_lnum: LinenrT,
    /// `true` with `":w >>file"` command (`append`).
    pub append: bool,
    /// `true` with `":w !command"` and `":r!command"` (`usefilter`).
    pub usefilter: bool,
    /// Number of `'>'` or `'<'` for shift command (`amount`).
    pub amount: i32,
    /// Register name, NUL if none (`regname`).
    pub regname: i32,
    /// 0, [`FORCE_BIN`] or [`FORCE_NOBIN`] (`force_bin`).
    pub force_bin: i32,
    /// `++edit` argument (`read_edit`).
    pub read_edit: bool,
    /// `++p` argument (`mkdir_p`).
    pub mkdir_p: bool,
    /// `++ff=` argument (first char of argument) (`force_ff`).
    pub force_ff: u8,
    /// `++enc=` argument (index in `cmd[]`) (`force_enc`).
    pub force_enc: i32,
    /// [`BAD_KEEP`], [`BAD_DROP`] or replacement byte (`bad_char`).
    pub bad_char: i32,
    /// User command index (`useridx`).
    pub useridx: i32,
    /// Returned error message (`errmsg`).
    pub errmsg: Option<Vec<u8>>,
    /// Function used to get the next line (`ea_getline`).
    pub ea_getline: Option<LineGetter>,
    /// Argument for `ea_getline()` (`cookie`).
    pub cookie: *mut std::ffi::c_void,
    /// Condition stack for `":if"` etc. (`cstack`).
    pub cstack: *mut CstackT,
    /// Special character handling in command args: file part
    /// (`magic.file`).
    pub magic_file: bool,
    /// Special character handling in command args: `|` bar
    /// (`magic.bar`).
    pub magic_bar: bool,
}

impl Default for ExargT {
    fn default() -> Self {
        ExargT {
            arg: None,
            args: Vec::new(),
            nextcmd: None,
            cmd: None,
            cmdlinep: std::ptr::null_mut(),
            cmdline_tofree: None,
            // No real "unset" sentinel exists in the original (a real
            // exarg_T always gets its cmdidx from command-line
            // parsing) - CMD_SIZE (one past the last real command) is
            // used here as an unambiguous "not yet identified" marker,
            // matching this crate's preference for an out-of-range
            // sentinel over silently defaulting to CMD_append.
            cmdidx: CmdIdxT::SIZE,
            argt: 0,
            skip: false,
            forceit: false,
            addr_count: 0,
            line1: 0,
            line2: 0,
            addr_type: CmdAddrT::default(),
            flags: 0,
            do_ecmd_cmd: None,
            do_ecmd_lnum: 0,
            append: false,
            usefilter: false,
            amount: 0,
            regname: 0,
            force_bin: 0,
            read_edit: false,
            mkdir_p: false,
            force_ff: 0,
            force_enc: 0,
            bad_char: 0,
            useridx: 0,
            errmsg: None,
            ea_getline: None,
            cookie: std::ptr::null_mut(),
            cstack: std::ptr::null_mut(),
            magic_file: false,
            magic_bar: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ex_files_combines_xfile_and_extra() {
        assert_eq!(ex_flags::FILES, ex_flags::XFILE | ex_flags::EXTRA);
        assert_eq!(ex_flags::FILE1, ex_flags::FILES | ex_flags::NOSPC);
        assert_eq!(ex_flags::WORD1, ex_flags::EXTRA | ex_flags::NOSPC);
    }

    #[test]
    fn cmd_addr_default_is_lines() {
        assert_eq!(CmdAddrT::default(), CmdAddrT::Lines);
    }

    #[test]
    fn bad_char_constants_match_c_macros() {
        assert_eq!(BAD_REPLACE, b'?');
        assert_eq!(BAD_KEEP, -1);
        assert_eq!(BAD_DROP, -2);
    }

    #[test]
    fn cmdmod_default_is_zeroed() {
        let cm = CmdmodT::default();
        assert_eq!(cm.cmod_flags, 0);
        assert!(cm.cmod_filter_pat.is_none());
        assert!(!cm.cmod_filter_force);
    }

    #[test]
    fn sub_replacement_string_default_has_no_previous_sub() {
        let s = SubReplacementString::default();
        assert!(s.sub.is_none());
        assert!(s.additional_data.is_none());
    }

    #[test]
    fn cmdidx_special_sentinels_match_c_macros() {
        assert_eq!(CmdIdxT::USER as i32, -1);
        assert_eq!(CmdIdxT::USER_BUF as i32, -2);
    }

    #[test]
    fn cmdidx_first_and_last_real_commands_match_c_enum() {
        assert_eq!(CmdIdxT::append as i32, 0);
        assert_eq!(CmdIdxT::abbreviate as i32, 1);
        assert_eq!(CmdIdxT::Next as i32, 560);
        assert_eq!(CmdIdxT::SIZE as i32, 561);
    }

    #[test]
    fn cmdidx_command_count_matches_ex_cmds_defs_generated_h() {
        // ex_cmds_defs.generated.h's own `command_count = 561` -
        // CMD_SIZE (one past the last real command) must equal it.
        assert_eq!(CmdIdxT::SIZE as i32, 561);
    }

    #[test]
    fn cmdidx_next_and_capital_next_are_genuinely_distinct_commands() {
        // `:next` (ex_next) and `:Next` (ex_previous) are two
        // DIFFERENT real Vim commands distinguished only by case -
        // confirmed directly against ex_cmds_defs.generated.h. A naive
        // "capitalize first letter" transform would have collided
        // these; exact-casing preservation (this module's own doc
        // comment) keeps them distinct.
        assert_eq!(CmdIdxT::next as i32, 291);
        assert_eq!(CmdIdxT::Next as i32, 560);
        assert_ne!(CmdIdxT::next as i32, CmdIdxT::Next as i32);
    }

    #[test]
    fn cmdidx_keyword_colliding_names_are_reachable_via_raw_identifiers() {
        // Spot-check every one of the 12 Rust-keyword-colliding
        // command names compiles and resolves to its real index.
        assert_eq!(CmdIdxT::r#break as i32, 34);
        assert_eq!(CmdIdxT::r#continue as i32, 96);
        assert_eq!(CmdIdxT::r#const as i32, 99);
        assert_eq!(CmdIdxT::r#else as i32, 140);
        assert_eq!(CmdIdxT::r#for as i32, 167);
        assert_eq!(CmdIdxT::r#if as i32, 187);
        assert_eq!(CmdIdxT::r#let as i32, 231);
        assert_eq!(CmdIdxT::r#move as i32, 273);
        assert_eq!(CmdIdxT::r#match as i32, 279);
        assert_eq!(CmdIdxT::r#return as i32, 374);
        assert_eq!(CmdIdxT::r#try as i32, 491);
        assert_eq!(CmdIdxT::r#while as i32, 529);
    }

    #[test]
    fn exarg_default_is_zeroed_with_no_command_selected() {
        let ea = ExargT::default();
        assert!(ea.arg.is_none());
        assert!(ea.args.is_empty());
        assert!(ea.nextcmd.is_none());
        assert!(ea.cmd.is_none());
        assert!(ea.cmdlinep.is_null());
        assert_eq!(ea.cmdidx, CmdIdxT::SIZE);
        assert_eq!(ea.argt, 0);
        assert!(!ea.skip);
        assert!(!ea.forceit);
        assert_eq!(ea.addr_count, 0);
        assert_eq!(ea.line1, 0);
        assert_eq!(ea.line2, 0);
        assert_eq!(ea.addr_type, CmdAddrT::Lines);
        assert!(ea.errmsg.is_none());
        assert!(ea.ea_getline.is_none());
        assert!(ea.cookie.is_null());
        assert!(ea.cstack.is_null());
        assert!(!ea.magic_file);
        assert!(!ea.magic_bar);
    }

    #[test]
    fn command_definition_can_be_constructed_with_no_callbacks() {
        let cmd = CommandDefinition {
            cmd_name: b"next",
            cmd_func: None,
            cmd_preview_func: None,
            cmd_argt: ex_flags::RANGE | ex_flags::COUNT,
            cmd_addr_type: CmdAddrT::Other,
        };
        assert_eq!(cmd.cmd_name, b"next");
        assert!(cmd.cmd_func.is_none());
        assert!(cmd.cmd_preview_func.is_none());
    }
}
