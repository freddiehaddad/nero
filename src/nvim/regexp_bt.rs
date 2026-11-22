#![forbid(unsafe_code)]

//! Backtracking regular expression engine (`src/nvim/regexp.c`).

use crate::message::semsg;
use std::mem::size_of;

use super::*;

const END: i32 = 0;
const BOL: i32 = 1;
const EOL: i32 = 2;
const BRANCH: i32 = 3;
const BACK: i32 = 4;
const EXACTLY: i32 = 5;
const NOTHING: i32 = 6;
const STAR: i32 = 7;
const PLUS: i32 = 8;
const MATCH: i32 = 9;
const NOMATCH: i32 = 10;
const BEHIND: i32 = 11;
const NOBEHIND: i32 = 12;
const SUBPAT: i32 = 13;
const BRACE_SIMPLE: i32 = 14;
const BOW: i32 = 15;
const EOW: i32 = 16;
const BRACE_LIMITS: i32 = 17;
const NEWL: i32 = 18;
const BHPOS: i32 = 19;

const ADD_NL: i32 = 30;
const ANY: i32 = 20;
const ANYOF: i32 = 21;
const ANYBUT: i32 = 22;
const IDENT: i32 = 23;
const SIDENT: i32 = 24;
const KWORD: i32 = 25;
const SKWORD: i32 = 26;
const FNAME: i32 = 27;
const SFNAME: i32 = 28;
const PRINT: i32 = 29;
const SPRINT: i32 = 30;
const WHITE: i32 = 31;
const NWHITE: i32 = 32;
const DIGIT: i32 = 33;
const NDIGIT: i32 = 34;
const HEX: i32 = 35;
const NHEX: i32 = 36;
const OCTAL: i32 = 37;
const NOCTAL: i32 = 38;
const WORD: i32 = 39;
const NWORD: i32 = 40;
const HEAD: i32 = 41;
const NHEAD: i32 = 42;
const ALPHA: i32 = 43;
const NALPHA: i32 = 44;
const LOWER: i32 = 45;
const NLOWER: i32 = 46;
const UPPER: i32 = 47;
const NUPPER: i32 = 48;
const FIRST_NL: i32 = ANY + ADD_NL;
const LAST_NL: i32 = NUPPER + ADD_NL;

const MOPEN: i32 = 80;
const MCLOSE: i32 = 90;
const BACKREF: i32 = 100;
const ZOPEN: i32 = 110;
const ZCLOSE: i32 = 120;
const ZREF: i32 = 130;
const BRACE_COMPLEX: i32 = 140;
const NOPEN: i32 = 150;
const NCLOSE: i32 = 151;

const MULTIBYTECODE: i32 = 200;
const RE_BOF: i32 = 201;
const RE_EOF: i32 = 202;
const CURSOR: i32 = 203;
const RE_LNUM: i32 = 204;
const RE_COL: i32 = 205;
const RE_VCOL: i32 = 206;
const RE_MARK: i32 = 207;
const RE_VISUAL: i32 = 208;
const RE_COMPOSING: i32 = 209;

const HASWIDTH: i32 = 0x1;
const SIMPLE: i32 = 0x2;
const SPSTART: i32 = 0x4;
const HASNL: i32 = 0x8;
const HASLOOKBH: i32 = 0x10;
const WORST: i32 = 0;

const REGSTACK_INITIAL: i32 = 2048;
const BACKPOS_INITIAL: i32 = 64;

static CLASSCODES: [i32; 27] = [
    ANY, IDENT, SIDENT, KWORD, SKWORD, FNAME, SFNAME, PRINT, SPRINT, WHITE, NWHITE, DIGIT, NDIGIT,
    HEX, NHEX, OCTAL, NOCTAL, WORD, NWORD, HEAD, NHEAD, ALPHA, NALPHA, LOWER, NLOWER, UPPER,
    NUPPER,
];

#[derive(Clone, Copy, Default)]
struct Regstar {
    nextb: i32,
    nextb_ic: i32,
    count: i64,
    minval: i64,
    #[expect(dead_code)]
    maxval: i64,
}

const JUST_CALC_SIZE: usize = usize::MAX;

#[derive(Clone, Copy)]
pub(super) struct Backpos {
    bp_scan: usize,
    bp_pos: Regsave,
}

struct RegexpBtState {
    reg_program: std::cell::RefCell<Vec<u8>>,
    regcode_idx: std::cell::Cell<usize>,
}

impl RegexpBtState {
    const fn new() -> Self {
        Self {
            reg_program: std::cell::RefCell::new(Vec::new()),
            regcode_idx: std::cell::Cell::new(JUST_CALC_SIZE),
        }
    }
}

std::thread_local! {
    static BT_STATE: RegexpBtState = const { RegexpBtState::new() };
}

#[inline(always)]
fn bt_st<R>(f: impl FnOnce(&RegexpBtState) -> R) -> R {
    BT_STATE.with(f)
}

#[inline(always)]
fn with_nl(op: i32) -> bool {
    (FIRST_NL..=LAST_NL).contains(&op)
}

#[inline(always)]
fn op_fn(prog: &[u8], p: Option<usize>) -> i32 {
    p.and_then(|idx| prog.get(idx)).map_or(0, |&b| i32::from(b))
}

#[inline(always)]
fn next_fn(prog: &[u8], idx: usize) -> i32 {
    ((i32::from(prog[idx + 1]) & 0o377) << 8) + (i32::from(prog[idx + 2]) & 0o377)
}

#[inline(always)]
fn operand_idx(idx: usize) -> usize {
    idx + 3
}

/// Whether node `idx` is the END of the whole pattern: the last node of the program, after which
/// its operand would start. Look-around operands end in earlier END nodes.
#[inline(always)]
fn is_pattern_end(prog: &[u8], idx: usize) -> bool {
    operand_idx(idx) == prog.len()
}

#[inline(always)]
fn operand_min(prog: &[u8], idx: usize) -> i64 {
    i64::from(i32::from_be_bytes([
        prog[idx + 3],
        prog[idx + 4],
        prog[idx + 5],
        prog[idx + 6],
    ]))
}

#[inline(always)]
fn operand_max(prog: &[u8], idx: usize) -> i64 {
    operand_min(prog, idx + 4)
}

#[inline(always)]
fn operand_cmp(prog: &[u8], idx: usize) -> u8 {
    prog[idx + 7]
}

/// Moves the current position past the character there (upstream's ADVANCE_REGINPUT()).
#[inline(always)]
fn advance_reginput(rex: &mut Regexec<'_>) {
    let step = utfc_ptr2len(rex.rest()) as usize;
    rex.advance_input(step);
}

fn bt_recognize_char_class(slice: &[u8]) -> i32 {
    let mut newl = false;
    let config = get_char_class_bits(slice, &mut newl);
    if config < 0 || newl {
        return 0;
    }
    if config == CLASS_O9 {
        DIGIT
    } else if config == (CLASS_NOT | CLASS_O9) {
        NDIGIT
    } else if config == (CLASS_AF_CONST | CLASS_AF | CLASS_O9) {
        HEX
    } else if config == (CLASS_NOT | CLASS_AF_CONST | CLASS_AF | CLASS_O9) {
        NHEX
    } else if config == CLASS_O7 {
        OCTAL
    } else if config == (CLASS_NOT | CLASS_O7) {
        NOCTAL
    } else if config == (CLASS_AZ_CONST | CLASS_AZ | CLASS_O9 | CLASS_UNDERSCORE) {
        WORD
    } else if config == (CLASS_NOT | CLASS_AZ_CONST | CLASS_AZ | CLASS_O9 | CLASS_UNDERSCORE) {
        NWORD
    } else if config == (CLASS_AZ_CONST | CLASS_AZ | CLASS_UNDERSCORE) {
        HEAD
    } else if config == (CLASS_NOT | CLASS_AZ_CONST | CLASS_AZ | CLASS_UNDERSCORE) {
        NHEAD
    } else if config == (CLASS_AZ_CONST | CLASS_AZ) {
        ALPHA
    } else if config == (CLASS_NOT | CLASS_AZ_CONST | CLASS_AZ) {
        NALPHA
    } else {
        0
    }
}

fn use_multibytecode(c: i32) -> bool {
    utf_char2len(c) > 1 && (re_multi_type(peekchr()) != NOT_MULTI || utf_iscomposing_legacy(c))
}

fn regc(b: i32) {
    let idx = bt_st(|st| st.regcode_idx.get());
    if idx == JUST_CALC_SIZE {
        RE_STATE.with(|s| s.regsize.set(s.regsize.get() + (1)));
    } else {
        bt_st(|st| {
            st.reg_program.borrow_mut()[idx] = b as u8;
            st.regcode_idx.set(idx + 1);
        });
    }
}

fn regmbc(c: i32) {
    let idx = bt_st(|st| st.regcode_idx.get());
    if idx == JUST_CALC_SIZE {
        RE_STATE.with(|s| {
            s.regsize
                .set(s.regsize.get() + (i64::from(utf_char2len(c))))
        });
    } else {
        bt_st(|st| {
            let len = crate::mbyte::utf_char2slice(c, &mut st.reg_program.borrow_mut()[idx..]);
            st.regcode_idx.set(idx + len);
        });
    }
}

#[rustfmt::skip]
fn reg_equi_class(c: i32) {
    match c {
            0x41 | 0xc0 | 0xc1 | 0xc2 | 0xc3 | 0xc4 | 0xc5 | 0x100 | 0x102 | 0x104 | 0x1cd
            | 0x1de | 0x1e0 | 0x1fa | 0x202 | 0x226 | 0x23a | 0x1e00 | 0x1ea0 | 0x1ea2
            | 0x1ea4 | 0x1ea6 | 0x1ea8 | 0x1eaa | 0x1eac | 0x1eae | 0x1eb0 | 0x1eb2 | 0x1eb4
            | 0x1eb6 => {
                regmbc(b'A' as i32); regmbc(0xc0); regmbc(0xc1); regmbc(0xc2);
                regmbc(0xc3); regmbc(0xc4); regmbc(0xc5);
                regmbc(0x100); regmbc(0x102); regmbc(0x104);
                regmbc(0x1cd); regmbc(0x1de); regmbc(0x1e0);
                regmbc(0x1fa); regmbc(0x202); regmbc(0x226);
                regmbc(0x23a); regmbc(0x1e00); regmbc(0x1ea0);
                regmbc(0x1ea2); regmbc(0x1ea4); regmbc(0x1ea6);
                regmbc(0x1ea8); regmbc(0x1eaa); regmbc(0x1eac);
                regmbc(0x1eae); regmbc(0x1eb0); regmbc(0x1eb2);
                regmbc(0x1eb4); regmbc(0x1eb6);
                return;
            }
            0x42 | 0x181 | 0x243 | 0x1e02 | 0x1e04 | 0x1e06 => {
                regmbc(b'B' as i32);
                regmbc(0x181); regmbc(0x243); regmbc(0x1e02);
                regmbc(0x1e04); regmbc(0x1e06);
                return;
            }
            0x43 | 0xc7 | 0x106 | 0x108 | 0x10a | 0x10c | 0x187 | 0x23b | 0x1e08 | 0xa792 => {
                regmbc(b'C' as i32); regmbc(0xc7);
                regmbc(0x106); regmbc(0x108); regmbc(0x10a);
                regmbc(0x10c); regmbc(0x187); regmbc(0x23b);
                regmbc(0x1e08); regmbc(0xa792);
                return;
            }
            0x44 | 0x10e | 0x110 | 0x18a | 0x1e0a | 0x1e0c | 0x1e0e | 0x1e10 | 0x1e12 => {
                regmbc(b'D' as i32); regmbc(0x10e); regmbc(0x110);
                regmbc(0x18a); regmbc(0x1e0a); regmbc(0x1e0c);
                regmbc(0x1e0e); regmbc(0x1e10); regmbc(0x1e12);
                return;
            }
            0x45 | 0xc8 | 0xc9 | 0xca | 0xcb | 0x112 | 0x114 | 0x116 | 0x118 | 0x11a | 0x204
            | 0x206 | 0x228 | 0x246 | 0x1e14 | 0x1e16 | 0x1e18 | 0x1e1a | 0x1e1c | 0x1eb8
            | 0x1eba | 0x1ebc | 0x1ebe | 0x1ec0 | 0x1ec2 | 0x1ec4 | 0x1ec6 => {
                regmbc(b'E' as i32); regmbc(0xc8); regmbc(0xc9);
                regmbc(0xca); regmbc(0xcb); regmbc(0x112);
                regmbc(0x114); regmbc(0x116); regmbc(0x118);
                regmbc(0x11a); regmbc(0x204); regmbc(0x206);
                regmbc(0x228); regmbc(0x246); regmbc(0x1e14);
                regmbc(0x1e16); regmbc(0x1e18); regmbc(0x1e1a);
                regmbc(0x1e1c); regmbc(0x1eb8); regmbc(0x1eba);
                regmbc(0x1ebc); regmbc(0x1ebe); regmbc(0x1ec0);
                regmbc(0x1ec2); regmbc(0x1ec4); regmbc(0x1ec6);
                return;
            }
            0x46 | 0x191 | 0x1e1e | 0xa798 => {
                regmbc(b'F' as i32); regmbc(0x191); regmbc(0x1e1e);
                regmbc(0xa798);
                return;
            }
            0x47 | 0x11c | 0x11e | 0x120 | 0x122 | 0x193 | 0x1e4 | 0x1e6 | 0x1f4 | 0x1e20
            | 0xa7a0 => {
                regmbc(b'G' as i32); regmbc(0x11c); regmbc(0x11e);
                regmbc(0x120); regmbc(0x122); regmbc(0x193);
                regmbc(0x1e4); regmbc(0x1e6); regmbc(0x1f4);
                regmbc(0x1e20); regmbc(0xa7a0);
                return;
            }
            0x48 | 0x124 | 0x126 | 0x21e | 0x1e22 | 0x1e24 | 0x1e26 | 0x1e28 | 0x1e2a | 0x2c67 => {
                regmbc(b'H' as i32); regmbc(0x124); regmbc(0x126);
                regmbc(0x21e); regmbc(0x1e22); regmbc(0x1e24);
                regmbc(0x1e26); regmbc(0x1e28); regmbc(0x1e2a);
                regmbc(0x2c67);
                return;
            }
            0x49 | 0xcc | 0xcd | 0xce | 0xcf | 0x128 | 0x12a | 0x12c | 0x12e | 0x130 | 0x197
            | 0x1cf | 0x208 | 0x20a | 0x1e2c | 0x1e2e | 0x1ec8 | 0x1eca => {
                regmbc(b'I' as i32); regmbc(0xcc); regmbc(0xcd);
                regmbc(0xce); regmbc(0xcf); regmbc(0x128);
                regmbc(0x12a); regmbc(0x12c); regmbc(0x12e);
                regmbc(0x130); regmbc(0x197); regmbc(0x1cf);
                regmbc(0x208); regmbc(0x20a); regmbc(0x1e2c);
                regmbc(0x1e2e); regmbc(0x1ec8); regmbc(0x1eca);
                return;
            }
            0x4a | 0x134 | 0x248 => {
                regmbc(b'J' as i32); regmbc(0x134); regmbc(0x248);
                return;
            }
            0x4b | 0x136 | 0x198 | 0x1e8 | 0x1e30 | 0x1e32 | 0x1e34 | 0x2c69 | 0xa740 => {
                regmbc(b'K' as i32); regmbc(0x136); regmbc(0x198);
                regmbc(0x1e8); regmbc(0x1e30); regmbc(0x1e32);
                regmbc(0x1e34); regmbc(0x2c69); regmbc(0xa740);
                return;
            }
            0x4c | 0x139 | 0x13b | 0x13d | 0x13f | 0x141 | 0x23d | 0x1e36 | 0x1e38 | 0x1e3a
            | 0x1e3c | 0x2c60 => {
                regmbc(b'L' as i32); regmbc(0x139); regmbc(0x13b);
                regmbc(0x13d); regmbc(0x13f); regmbc(0x141);
                regmbc(0x23d); regmbc(0x1e36); regmbc(0x1e38);
                regmbc(0x1e3a); regmbc(0x1e3c); regmbc(0x2c60);
                return;
            }
            0x4d | 0x1e3e | 0x1e40 | 0x1e42 => {
                regmbc(b'M' as i32); regmbc(0x1e3e); regmbc(0x1e40);
                regmbc(0x1e42);
                return;
            }
            0x4e | 0xd1 | 0x143 | 0x145 | 0x147 | 0x1f8 | 0x1e44 | 0x1e46 | 0x1e48 | 0x1e4a
            | 0xa7a4 => {
                regmbc(b'N' as i32); regmbc(0xd1);
                regmbc(0x143); regmbc(0x145); regmbc(0x147);
                regmbc(0x1f8); regmbc(0x1e44); regmbc(0x1e46);
                regmbc(0x1e48); regmbc(0x1e4a); regmbc(0xa7a4);
                return;
            }
            0x4f | 0xd2 | 0xd3 | 0xd4 | 0xd5 | 0xd6 | 0xd8 | 0x14c | 0x14e | 0x150 | 0x19f
            | 0x1a0 | 0x1d1 | 0x1ea | 0x1ec | 0x1fe | 0x20c | 0x20e | 0x22a | 0x22c | 0x22e
            | 0x230 | 0x1e4c | 0x1e4e | 0x1e50 | 0x1e52 | 0x1ecc | 0x1ece | 0x1ed0 | 0x1ed2
            | 0x1ed4 | 0x1ed6 | 0x1ed8 | 0x1eda | 0x1edc | 0x1ede | 0x1ee0 | 0x1ee2 => {
                regmbc(b'O' as i32); regmbc(0xd2); regmbc(0xd3); regmbc(0xd4);
                regmbc(0xd5); regmbc(0xd6); regmbc(0xd8);
                regmbc(0x14c); regmbc(0x14e); regmbc(0x150);
                regmbc(0x19f); regmbc(0x1a0); regmbc(0x1d1);
                regmbc(0x1ea); regmbc(0x1ec); regmbc(0x1fe);
                regmbc(0x20c); regmbc(0x20e); regmbc(0x22a);
                regmbc(0x22c); regmbc(0x22e); regmbc(0x230);
                regmbc(0x1e4c); regmbc(0x1e4e); regmbc(0x1e50);
                regmbc(0x1e52); regmbc(0x1ecc); regmbc(0x1ece);
                regmbc(0x1ed0); regmbc(0x1ed2); regmbc(0x1ed4);
                regmbc(0x1ed6); regmbc(0x1ed8); regmbc(0x1eda);
                regmbc(0x1edc); regmbc(0x1ede); regmbc(0x1ee0);
                regmbc(0x1ee2);
                return;
            }
            0x50 | 0x1a4 | 0x1e54 | 0x1e56 | 0x2c63 => {
                regmbc(b'P' as i32); regmbc(0x1a4); regmbc(0x1e54);
                regmbc(0x1e56); regmbc(0x2c63);
                return;
            }
            0x51 | 0x24a => {
                regmbc(b'Q' as i32); regmbc(0x24a);
                return;
            }
            0x52 | 0x154 | 0x156 | 0x158 | 0x210 | 0x212 | 0x24c | 0x1e58 | 0x1e5a | 0x1e5c
            | 0x1e5e | 0x2c64 | 0xa7a6 => {
                regmbc(b'R' as i32); regmbc(0x154); regmbc(0x156);
                regmbc(0x210); regmbc(0x212); regmbc(0x158);
                regmbc(0x24c); regmbc(0x1e58); regmbc(0x1e5a);
                regmbc(0x1e5c); regmbc(0x1e5e); regmbc(0x2c64);
                regmbc(0xa7a6);
                return;
            }
            0x53 | 0x15a | 0x15c | 0x15e | 0x160 | 0x218 | 0x1e60 | 0x1e62 | 0x1e64 | 0x1e66
            | 0x1e68 | 0x2c7e | 0xa7a8 => {
                regmbc(b'S' as i32); regmbc(0x15a); regmbc(0x15c);
                regmbc(0x15e); regmbc(0x160); regmbc(0x218);
                regmbc(0x1e60); regmbc(0x1e62); regmbc(0x1e64);
                regmbc(0x1e66); regmbc(0x1e68); regmbc(0x2c7e);
                regmbc(0xa7a8);
                return;
            }
            0x54 | 0x162 | 0x164 | 0x166 | 0x1ac | 0x1ae | 0x21a | 0x23e | 0x1e6a | 0x1e6c
            | 0x1e6e | 0x1e70 => {
                regmbc(b'T' as i32); regmbc(0x162); regmbc(0x164);
                regmbc(0x166); regmbc(0x1ac); regmbc(0x23e);
                regmbc(0x1ae); regmbc(0x21a); regmbc(0x1e6a);
                regmbc(0x1e6c); regmbc(0x1e6e); regmbc(0x1e70);
                return;
            }
            0x55 | 0xd9 | 0xda | 0xdb | 0xdc | 0x168 | 0x16a | 0x16c | 0x16e | 0x170 | 0x172
            | 0x1af | 0x1d3 | 0x1d5 | 0x1d7 | 0x1d9 | 0x1db | 0x214 | 0x216 | 0x244 | 0x1e72
            | 0x1e74 | 0x1e76 | 0x1e78 | 0x1e7a | 0x1ee4 | 0x1ee6 | 0x1ee8 | 0x1eea | 0x1eec
            | 0x1eee | 0x1ef0 => {
                regmbc(b'U' as i32); regmbc(0xd9); regmbc(0xda);
                regmbc(0xdb); regmbc(0xdc); regmbc(0x168);
                regmbc(0x16a); regmbc(0x16c); regmbc(0x16e);
                regmbc(0x170); regmbc(0x172); regmbc(0x1af);
                regmbc(0x1d3); regmbc(0x1d5); regmbc(0x1d7);
                regmbc(0x1d9); regmbc(0x1db); regmbc(0x214);
                regmbc(0x216); regmbc(0x244); regmbc(0x1e72);
                regmbc(0x1e74); regmbc(0x1e76); regmbc(0x1e78);
                regmbc(0x1e7a); regmbc(0x1ee4); regmbc(0x1ee6);
                regmbc(0x1ee8); regmbc(0x1eea); regmbc(0x1eec);
                regmbc(0x1eee); regmbc(0x1ef0);
                return;
            }
            0x56 | 0x1b2 | 0x1e7c | 0x1e7e => {
                regmbc(b'V' as i32); regmbc(0x1b2); regmbc(0x1e7c);
                regmbc(0x1e7e);
                return;
            }
            0x57 | 0x174 | 0x1e80 | 0x1e82 | 0x1e84 | 0x1e86 | 0x1e88 => {
                regmbc(b'W' as i32); regmbc(0x174); regmbc(0x1e80);
                regmbc(0x1e82); regmbc(0x1e84); regmbc(0x1e86);
                regmbc(0x1e88);
                return;
            }
            0x58 | 0x1e8a | 0x1e8c => {
                regmbc(b'X' as i32); regmbc(0x1e8a); regmbc(0x1e8c);
                return;
            }
            0x59 | 0xdd | 0x176 | 0x178 | 0x1b3 | 0x232 | 0x24e | 0x1e8e | 0x1ef2 | 0x1ef4
            | 0x1ef6 | 0x1ef8 => {
                regmbc(b'Y' as i32); regmbc(0xdd); regmbc(0x176);
                regmbc(0x178); regmbc(0x1b3); regmbc(0x232);
                regmbc(0x24e); regmbc(0x1e8e); regmbc(0x1ef2);
                regmbc(0x1ef4); regmbc(0x1ef6); regmbc(0x1ef8);
                return;
            }
            0x5a | 0x179 | 0x17b | 0x17d | 0x1b5 | 0x1e90 | 0x1e92 | 0x1e94 | 0x2c6b => {
                regmbc(b'Z' as i32); regmbc(0x179); regmbc(0x17b);
                regmbc(0x17d); regmbc(0x1b5); regmbc(0x1e90);
                regmbc(0x1e92); regmbc(0x1e94); regmbc(0x2c6b);
                return;
            }
            0x61 | 0xe0 | 0xe1 | 0xe2 | 0xe3 | 0xe4 | 0xe5 | 0x101 | 0x103 | 0x105 | 0x1ce
            | 0x1df | 0x1e1 | 0x1fb | 0x201 | 0x203 | 0x227 | 0x1d8f | 0x1e01 | 0x1e9a
            | 0x1ea1 | 0x1ea3 | 0x1ea5 | 0x1ea7 | 0x1ea9 | 0x1eab | 0x1ead | 0x1eaf | 0x1eb1
            | 0x1eb3 | 0x1eb5 | 0x1eb7 | 0x2c65 => {
                regmbc(b'a' as i32); regmbc(0xe0); regmbc(0xe1);
                regmbc(0xe2); regmbc(0xe3); regmbc(0xe4);
                regmbc(0xe5); regmbc(0x101); regmbc(0x103);
                regmbc(0x105); regmbc(0x1ce); regmbc(0x1df);
                regmbc(0x1e1); regmbc(0x1fb); regmbc(0x201);
                regmbc(0x203); regmbc(0x227); regmbc(0x1d8f);
                regmbc(0x1e01); regmbc(0x1e9a); regmbc(0x1ea1);
                regmbc(0x1ea3); regmbc(0x1ea5); regmbc(0x1ea7);
                regmbc(0x1ea9); regmbc(0x1eab); regmbc(0x1ead);
                regmbc(0x1eaf); regmbc(0x1eb1); regmbc(0x1eb3);
                regmbc(0x1eb5); regmbc(0x1eb7); regmbc(0x2c65);
                return;
            }
            0x62 | 0x180 | 0x253 | 0x1d6c | 0x1d80 | 0x1e03 | 0x1e05 | 0x1e07 => {
                regmbc(b'b' as i32);
                regmbc(0x180); regmbc(0x253); regmbc(0x1d6c);
                regmbc(0x1d80); regmbc(0x1e03); regmbc(0x1e05);
                regmbc(0x1e07);
                return;
            }
            0x63 | 0xe7 | 0x107 | 0x109 | 0x10b | 0x10d | 0x188 | 0x23c | 0x1e09 | 0xa793
            | 0xa794 => {
                regmbc(b'c' as i32); regmbc(0xe7); regmbc(0x107);
                regmbc(0x109); regmbc(0x10b); regmbc(0x10d);
                regmbc(0x188); regmbc(0x23c); regmbc(0x1e09);
                regmbc(0xa793); regmbc(0xa794);
                return;
            }
            0x64 | 0x10f | 0x111 | 0x257 | 0x1d6d | 0x1d81 | 0x1d91 | 0x1e0b | 0x1e0d | 0x1e0f
            | 0x1e11 | 0x1e13 => {
                regmbc(b'd' as i32); regmbc(0x10f); regmbc(0x111);
                regmbc(0x257); regmbc(0x1d6d); regmbc(0x1d81);
                regmbc(0x1d91); regmbc(0x1e0b); regmbc(0x1e0d);
                regmbc(0x1e0f); regmbc(0x1e11); regmbc(0x1e13);
                return;
            }
            0x65 | 0xe8 | 0xe9 | 0xea | 0xeb | 0x113 | 0x115 | 0x117 | 0x119 | 0x11b | 0x205
            | 0x207 | 0x229 | 0x247 | 0x1d92 | 0x1e15 | 0x1e17 | 0x1e19 | 0x1e1b | 0x1e1d
            | 0x1eb9 | 0x1ebb | 0x1ebd | 0x1ebf | 0x1ec1 | 0x1ec3 | 0x1ec5 | 0x1ec7 => {
                regmbc(b'e' as i32); regmbc(0xe8); regmbc(0xe9);
                regmbc(0xea); regmbc(0xeb); regmbc(0x113);
                regmbc(0x115); regmbc(0x117); regmbc(0x119);
                regmbc(0x11b); regmbc(0x205); regmbc(0x207);
                regmbc(0x229); regmbc(0x247); regmbc(0x1d92);
                regmbc(0x1e15); regmbc(0x1e17); regmbc(0x1e19);
                regmbc(0x1e1b); regmbc(0x1e1d); regmbc(0x1eb9);
                regmbc(0x1ebb); regmbc(0x1ebd); regmbc(0x1ebf);
                regmbc(0x1ec1); regmbc(0x1ec3); regmbc(0x1ec5);
                regmbc(0x1ec7);
                return;
            }
            0x66 | 0x192 | 0x1d6e | 0x1d82 | 0x1e1f | 0xa799 => {
                regmbc(b'f' as i32); regmbc(0x192); regmbc(0x1d6e);
                regmbc(0x1d82); regmbc(0x1e1f); regmbc(0xa799);
                return;
            }
            0x67 | 0x11d | 0x11f | 0x121 | 0x123 | 0x1e5 | 0x1e7 | 0x1f5 | 0x260 | 0x1d83
            | 0x1e21 | 0xa7a1 => {
                regmbc(b'g' as i32); regmbc(0x11d); regmbc(0x11f);
                regmbc(0x121); regmbc(0x123); regmbc(0x1e5);
                regmbc(0x1e7); regmbc(0x1f5); regmbc(0x260);
                regmbc(0x1d83); regmbc(0x1e21); regmbc(0xa7a1);
                return;
            }
            0x68 | 0x125 | 0x127 | 0x21f | 0x1e23 | 0x1e25 | 0x1e27 | 0x1e29 | 0x1e2b | 0x1e96
            | 0x2c68 | 0xa795 => {
                regmbc(b'h' as i32); regmbc(0x125); regmbc(0x127);
                regmbc(0x21f); regmbc(0x1e23); regmbc(0x1e25);
                regmbc(0x1e27); regmbc(0x1e29); regmbc(0x1e2b);
                regmbc(0x1e96); regmbc(0x2c68); regmbc(0xa795);
                return;
            }
            0x69 | 0xec | 0xed | 0xee | 0xef | 0x129 | 0x12b | 0x12d | 0x12f | 0x1d0 | 0x209
            | 0x20b | 0x268 | 0x1d96 | 0x1e2d | 0x1e2f | 0x1ec9 | 0x1ecb => {
                regmbc(b'i' as i32); regmbc(0xec); regmbc(0xed);
                regmbc(0xee); regmbc(0xef); regmbc(0x129);
                regmbc(0x12b); regmbc(0x12d); regmbc(0x12f);
                regmbc(0x1d0); regmbc(0x209); regmbc(0x20b);
                regmbc(0x268); regmbc(0x1d96); regmbc(0x1e2d);
                regmbc(0x1e2f); regmbc(0x1ec9); regmbc(0x1ecb);
                return;
            }
            0x6a | 0x135 | 0x1f0 | 0x249 => {
                regmbc(b'j' as i32); regmbc(0x135); regmbc(0x1f0);
                regmbc(0x249);
                return;
            }
            0x6b | 0x137 | 0x199 | 0x1e9 | 0x1d84 | 0x1e31 | 0x1e33 | 0x1e35 | 0x2c6a | 0xa741 => {
                regmbc(b'k' as i32); regmbc(0x137); regmbc(0x199);
                regmbc(0x1e9); regmbc(0x1d84); regmbc(0x1e31);
                regmbc(0x1e33); regmbc(0x1e35); regmbc(0x2c6a);
                regmbc(0xa741);
                return;
            }
            0x6c | 0x13a | 0x13c | 0x13e | 0x140 | 0x142 | 0x19a | 0x1e37 | 0x1e39 | 0x1e3b
            | 0x1e3d | 0x2c61 => {
                regmbc(b'l' as i32); regmbc(0x13a); regmbc(0x13c);
                regmbc(0x13e); regmbc(0x140); regmbc(0x142);
                regmbc(0x19a); regmbc(0x1e37); regmbc(0x1e39);
                regmbc(0x1e3b); regmbc(0x1e3d); regmbc(0x2c61);
                return;
            }
            0x6d | 0x1d6f | 0x1e3f | 0x1e41 | 0x1e43 => {
                regmbc(b'm' as i32); regmbc(0x1d6f); regmbc(0x1e3f);
                regmbc(0x1e41); regmbc(0x1e43);
                return;
            }
            0x6e | 0xf1 | 0x144 | 0x146 | 0x148 | 0x149 | 0x1f9 | 0x1d70 | 0x1d87 | 0x1e45
            | 0x1e47 | 0x1e49 | 0x1e4b | 0xa7a5 => {
                regmbc(b'n' as i32); regmbc(0xf1); regmbc(0x144);
                regmbc(0x146); regmbc(0x148); regmbc(0x149);
                regmbc(0x1f9); regmbc(0x1d70); regmbc(0x1d87);
                regmbc(0x1e45); regmbc(0x1e47); regmbc(0x1e49);
                regmbc(0x1e4b); regmbc(0xa7a5);
                return;
            }
            0x6f | 0xf2 | 0xf3 | 0xf4 | 0xf5 | 0xf6 | 0xf8 | 0x14d | 0x14f | 0x151 | 0x1a1
            | 0x1d2 | 0x1eb | 0x1ed | 0x1ff | 0x20d | 0x20f | 0x22b | 0x22d | 0x22f | 0x231
            | 0x275 | 0x1e4d | 0x1e4f | 0x1e51 | 0x1e53 | 0x1ecd | 0x1ecf | 0x1ed1 | 0x1ed3
            | 0x1ed5 | 0x1ed7 | 0x1ed9 | 0x1edb | 0x1edd | 0x1edf | 0x1ee1 | 0x1ee3 => {
                regmbc(b'o' as i32); regmbc(0xf2); regmbc(0xf3);
                regmbc(0xf4); regmbc(0xf5); regmbc(0xf6);
                regmbc(0xf8); regmbc(0x14d); regmbc(0x14f);
                regmbc(0x151); regmbc(0x1a1); regmbc(0x1d2);
                regmbc(0x1eb); regmbc(0x1ed); regmbc(0x1ff);
                regmbc(0x20d); regmbc(0x20f); regmbc(0x22b);
                regmbc(0x22d); regmbc(0x22f); regmbc(0x231);
                regmbc(0x275); regmbc(0x1e4d); regmbc(0x1e4f);
                regmbc(0x1e51); regmbc(0x1e53); regmbc(0x1ecd);
                regmbc(0x1ecf); regmbc(0x1ed1); regmbc(0x1ed3);
                regmbc(0x1ed5); regmbc(0x1ed7); regmbc(0x1ed9);
                regmbc(0x1edb); regmbc(0x1edd); regmbc(0x1edf);
                regmbc(0x1ee1); regmbc(0x1ee3);
                return;
            }
            0x70 | 0x1a5 | 0x1d71 | 0x1d7d | 0x1d88 | 0x1e55 | 0x1e57 => {
                regmbc(b'p' as i32); regmbc(0x1a5); regmbc(0x1d71);
                regmbc(0x1d7d); regmbc(0x1d88); regmbc(0x1e55);
                regmbc(0x1e57);
                return;
            }
            0x71 | 0x24b | 0x2a0 => {
                regmbc(b'q' as i32); regmbc(0x24b); regmbc(0x2a0);
                return;
            }
            0x72 | 0x155 | 0x157 | 0x159 | 0x211 | 0x213 | 0x24d | 0x27d | 0x1d72 | 0x1d73
            | 0x1d89 | 0x1e59 | 0x1e5b | 0x1e5d | 0x1e5f | 0xa7a7 => {
                regmbc(b'r' as i32); regmbc(0x155); regmbc(0x157);
                regmbc(0x159); regmbc(0x211); regmbc(0x213);
                regmbc(0x24d); regmbc(0x1d72); regmbc(0x1d73);
                regmbc(0x1d89); regmbc(0x1e59); regmbc(0x27d);
                regmbc(0x1e5b); regmbc(0x1e5d); regmbc(0x1e5f);
                regmbc(0xa7a7);
                return;
            }
            0x73 | 0x15b | 0x15d | 0x15f | 0x161 | 0x219 | 0x23f | 0x1d74 | 0x1d8a | 0x1e61
            | 0x1e63 | 0x1e65 | 0x1e67 | 0x1e69 | 0xa7a9 => {
                regmbc(b's' as i32); regmbc(0x15b); regmbc(0x15d);
                regmbc(0x15f); regmbc(0x161); regmbc(0x23f);
                regmbc(0x219); regmbc(0x1d74); regmbc(0x1d8a);
                regmbc(0x1e61); regmbc(0x1e63); regmbc(0x1e65);
                regmbc(0x1e67); regmbc(0x1e69); regmbc(0xa7a9);
                return;
            }
            0x74 | 0x163 | 0x165 | 0x167 | 0x1ab | 0x1ad | 0x21b | 0x288 | 0x1d75 | 0x1e6b
            | 0x1e6d | 0x1e6f | 0x1e71 | 0x1e97 | 0x2c66 => {
                regmbc(b't' as i32); regmbc(0x163); regmbc(0x165);
                regmbc(0x167); regmbc(0x1ab); regmbc(0x21b);
                regmbc(0x1ad); regmbc(0x288); regmbc(0x1d75);
                regmbc(0x1e6b); regmbc(0x1e6d); regmbc(0x1e6f);
                regmbc(0x1e71); regmbc(0x1e97); regmbc(0x2c66);
                return;
            }
            0x75 | 0xf9 | 0xfa | 0xfb | 0xfc | 0x169 | 0x16b | 0x16d | 0x16f | 0x171 | 0x173
            | 0x1b0 | 0x1d4 | 0x1d6 | 0x1d8 | 0x1da | 0x1dc | 0x215 | 0x217 | 0x289 | 0x1d7e
            | 0x1d99 | 0x1e73 | 0x1e75 | 0x1e77 | 0x1e79 | 0x1e7b | 0x1ee5 | 0x1ee7 | 0x1ee9
            | 0x1eeb | 0x1eed | 0x1eef | 0x1ef1 => {
                regmbc(b'u' as i32); regmbc(0xf9); regmbc(0xfa);
                regmbc(0xfb); regmbc(0xfc); regmbc(0x169);
                regmbc(0x16b); regmbc(0x16d); regmbc(0x16f);
                regmbc(0x171); regmbc(0x173); regmbc(0x1d6);
                regmbc(0x1d8); regmbc(0x1da); regmbc(0x1dc);
                regmbc(0x215); regmbc(0x217); regmbc(0x1b0);
                regmbc(0x1d4); regmbc(0x289); regmbc(0x1d7e);
                regmbc(0x1d99); regmbc(0x1e73); regmbc(0x1e75);
                regmbc(0x1e77); regmbc(0x1e79); regmbc(0x1e7b);
                regmbc(0x1ee5); regmbc(0x1ee7); regmbc(0x1ee9);
                regmbc(0x1eeb); regmbc(0x1eed); regmbc(0x1eef);
                regmbc(0x1ef1);
                return;
            }
            0x76 | 0x28b | 0x1d8c | 0x1e7d | 0x1e7f => {
                regmbc(b'v' as i32); regmbc(0x28b); regmbc(0x1d8c);
                regmbc(0x1e7d); regmbc(0x1e7f);
                return;
            }
            0x77 | 0x175 | 0x1e81 | 0x1e83 | 0x1e85 | 0x1e87 | 0x1e89 | 0x1e98 => {
                regmbc(b'w' as i32); regmbc(0x175); regmbc(0x1e81);
                regmbc(0x1e83); regmbc(0x1e85); regmbc(0x1e87);
                regmbc(0x1e89); regmbc(0x1e98);
                return;
            }
            0x78 | 0x1e8b | 0x1e8d => {
                regmbc(b'x' as i32); regmbc(0x1e8b); regmbc(0x1e8d);
                return;
            }
            0x79 | 0xfd | 0xff | 0x177 | 0x1b4 | 0x233 | 0x24f | 0x1e8f | 0x1e99 | 0x1ef3
            | 0x1ef5 | 0x1ef7 | 0x1ef9 => {
                regmbc(b'y' as i32); regmbc(0xfd); regmbc(0xff);
                regmbc(0x177); regmbc(0x1b4); regmbc(0x233);
                regmbc(0x24f); regmbc(0x1e8f); regmbc(0x1e99);
                regmbc(0x1ef3); regmbc(0x1ef5); regmbc(0x1ef7);
                regmbc(0x1ef9);
                return;
            }
            0x7a | 0x17a | 0x17c | 0x17e | 0x1b6 | 0x1d76 | 0x1d8e | 0x1e91 | 0x1e93 | 0x1e95
            | 0x2c6c => {
                regmbc(b'z' as i32); regmbc(0x17a); regmbc(0x17c);
                regmbc(0x17e); regmbc(0x1b6); regmbc(0x1d76);
                regmbc(0x1d8e); regmbc(0x1e91); regmbc(0x1e93);
                regmbc(0x1e95); regmbc(0x2c6c);
                return;
            }
            _ => {}
        }
        regmbc(c);
}

fn regnode(op: i32) -> Option<usize> {
    let ret = bt_st(|st| st.regcode_idx.get());
    if ret == JUST_CALC_SIZE {
        RE_STATE.with(|s| s.regsize.set(s.regsize.get() + (3)));
    } else {
        bt_st(|st| {
            let mut prog = st.reg_program.borrow_mut();
            prog[ret] = op as u8;
            prog[ret + 1] = NUL;
            prog[ret + 2] = NUL;
            st.regcode_idx.set(ret + 3);
        });
    }
    Some(ret)
}

fn re_put_uint32(prog: &mut [u8], p: usize, val: u32) -> usize {
    prog[p..p + 4].copy_from_slice(&val.to_be_bytes());
    p + 4
}

/// The node the "next" link of node `p` of `prog` points to.
fn regnext(prog: &[u8], p: Option<usize>) -> Option<usize> {
    let p = p?;
    if p == JUST_CALC_SIZE {
        return None;
    }
    let offset = next_fn(prog, p);
    if offset == 0 {
        return None;
    }
    if op_fn(prog, Some(p)) == BACK {
        Some(p - offset as usize)
    } else {
        Some(p + offset as usize)
    }
}

/// [`regnext`] in a program being compiled: once a link did not fit (`reg_toolong`), the
/// links are incomplete and are not followed.
fn regnext_compiling(prog: &[u8], p: Option<usize>) -> Option<usize> {
    if RE_STATE.with(|s| s.reg_toolong.get()) != 0 {
        return None;
    }
    regnext(prog, p)
}

fn regtail(p: Option<usize>, val: Option<usize>) {
    let Some(p) = p else {
        return;
    };
    if p == JUST_CALC_SIZE {
        return;
    }
    let val = val.unwrap();
    let mut scan = p;
    bt_st(|st| {
        let mut prog = st.reg_program.borrow_mut();
        loop {
            let temp = regnext_compiling(&prog, Some(scan));
            let Some(t) = temp else {
                break;
            };
            scan = t;
        }
        let offset = if op_fn(&prog, Some(scan)) == BACK {
            (scan as isize - val as isize) as i32
        } else {
            (val as isize - scan as isize) as i32
        };
        if offset > 0xffff {
            RE_STATE.with(|s| s.reg_toolong.set(1));
        } else {
            prog[scan + 1] = (((offset as u32) >> 8) & 0o377) as u8;
            prog[scan + 2] = (offset & 0o377) as u8;
        }
    });
}

fn regoptail(p: Option<usize>, val: Option<usize>) {
    let Some(p) = p else {
        return;
    };
    if p == JUST_CALC_SIZE {
        return;
    }
    let op = bt_st(|st| op_fn(&st.reg_program.borrow(), Some(p)));
    if op != BRANCH && !(BRACE_COMPLEX..=BRACE_COMPLEX + 9).contains(&op) {
        return;
    }
    regtail(Some(operand_idx(p)), val);
}

fn reginsert(op: i32, opnd: Option<usize>) {
    let end = bt_st(|st| st.regcode_idx.get());
    if end == JUST_CALC_SIZE {
        RE_STATE.with(|s| s.regsize.set(s.regsize.get() + (3)));
        return;
    }
    let opnd = opnd.unwrap();
    bt_st(|st| {
        st.regcode_idx.set(end + 3);
        let mut prog = st.reg_program.borrow_mut();
        prog.copy_within(opnd..end, opnd + 3);
        prog[opnd] = op as u8;
        prog[opnd + 1] = NUL;
        prog[opnd + 2] = NUL;
    });
}

fn reginsert_nr(op: i32, val: i64, opnd: Option<usize>) {
    let end = bt_st(|st| st.regcode_idx.get());
    if end == JUST_CALC_SIZE {
        RE_STATE.with(|s| s.regsize.set(s.regsize.get() + (7)));
        return;
    }
    let opnd = opnd.unwrap();
    bt_st(|st| {
        st.regcode_idx.set(end + 7);
        let mut prog = st.reg_program.borrow_mut();
        prog.copy_within(opnd..end, opnd + 7);
        prog[opnd] = op as u8;
        prog[opnd + 1] = NUL;
        prog[opnd + 2] = NUL;
        re_put_uint32(&mut prog, opnd + 3, val as u32);
    });
}

fn reginsert_limits(op: i32, minval: i64, maxval: i64, opnd: Option<usize>) {
    let end = bt_st(|st| st.regcode_idx.get());
    if end == JUST_CALC_SIZE {
        RE_STATE.with(|s| s.regsize.set(s.regsize.get() + (11)));
        return;
    }
    let opnd = opnd.unwrap();
    let place = bt_st(|st| {
        st.regcode_idx.set(end + 11);
        let mut prog = st.reg_program.borrow_mut();
        prog.copy_within(opnd..end, opnd + 11);
        prog[opnd] = op as u8;
        prog[opnd + 1] = NUL;
        prog[opnd + 2] = NUL;
        let place = re_put_uint32(&mut prog, opnd + 3, minval as u32);
        re_put_uint32(&mut prog, place, maxval as u32)
    });
    regtail(Some(opnd), Some(place));
}

fn regatom(flagp: &mut i32) -> Option<usize> {
    let mut ret: Option<usize>;
    let mut flags: i32 = 0;
    let mut extra: i32 = 0;
    let save_prev_at_start = RE_STATE.with(|s| s.prev_at_start.get());

    *flagp = WORST;

    let mut c = getchr();
    let mut goto_collection = false;
    let mut goto_multibyte = false;

    if c == magic_fn(b'^' as i32) {
        return regnode(BOL);
    } else if c == magic_fn(b'$' as i32) {
        RE_STATE.with(|s| s.had_eol.set(true));
        return regnode(EOL);
    } else if c == magic_fn(b'<' as i32) {
        return regnode(BOW);
    } else if c == magic_fn(b'>' as i32) {
        return regnode(EOW);
    } else if c == magic_fn(b'_' as i32) {
        c = no_magic(getchr());
        if c == b'^' as i32 {
            return regnode(BOL);
        }
        if c == b'$' as i32 {
            RE_STATE.with(|s| s.had_eol.set(true));
            return regnode(EOL);
        }
        extra = ADD_NL;
        *flagp |= HASNL;
        if c == b'[' as i32 {
            goto_collection = true;
        } else {
            // fallthrough to character classes
            let p = vim_strchr(CLASSCHARS, no_magic(c))
                .map_or(std::ptr::null_mut(), |s| s.as_ptr().cast_mut())
                .cast::<u8>();
            if p.is_null() {
                emsg(tr(E_INVALID_USE_OF_UNDERSCORE));
                msg_state().rc_did_emsg.set(true);
                return None;
            }
            if c == magic_fn(b'.' as i32) && utf_iscomposing_legacy(peekchr()) {
                c = getchr();
                goto_multibyte = true;
            } else {
                let idx = (p as usize) - (CLASSCHARS.as_ptr() as usize);
                ret = regnode(CLASSCODES[idx] + extra);
                *flagp |= HASWIDTH | SIMPLE;
                return ret;
            }
        }
    }

    if !goto_collection && !goto_multibyte {
        if matches!(
            c,
            x if x == magic_fn(b'.' as i32)
                || x == magic_fn(b'i' as i32)
                || x == magic_fn(b'I' as i32)
                || x == magic_fn(b'k' as i32)
                || x == magic_fn(b'K' as i32)
                || x == magic_fn(b'f' as i32)
                || x == magic_fn(b'F' as i32)
                || x == magic_fn(b'p' as i32)
                || x == magic_fn(b'P' as i32)
                || x == magic_fn(b's' as i32)
                || x == magic_fn(b'S' as i32)
                || x == magic_fn(b'd' as i32)
                || x == magic_fn(b'D' as i32)
                || x == magic_fn(b'x' as i32)
                || x == magic_fn(b'X' as i32)
                || x == magic_fn(b'o' as i32)
                || x == magic_fn(b'O' as i32)
                || x == magic_fn(b'w' as i32)
                || x == magic_fn(b'W' as i32)
                || x == magic_fn(b'h' as i32)
                || x == magic_fn(b'H' as i32)
                || x == magic_fn(b'a' as i32)
                || x == magic_fn(b'A' as i32)
                || x == magic_fn(b'l' as i32)
                || x == magic_fn(b'L' as i32)
                || x == magic_fn(b'u' as i32)
                || x == magic_fn(b'U' as i32)
        ) {
            let p = vim_strchr(CLASSCHARS, no_magic(c))
                .map_or(std::ptr::null_mut(), |s| s.as_ptr().cast_mut())
                .cast::<u8>();
            if p.is_null() {
                emsg(tr(E_INVALID_USE_OF_UNDERSCORE));
                msg_state().rc_did_emsg.set(true);
                return None;
            }
            if c == magic_fn(b'.' as i32) && utf_iscomposing_legacy(peekchr()) {
                c = getchr();
                goto_multibyte = true;
            } else {
                let idx = (p as usize) - (CLASSCHARS.as_ptr() as usize);
                ret = regnode(CLASSCODES[idx] + extra);
                *flagp |= HASWIDTH | SIMPLE;
                return ret;
            }
        } else if c == magic_fn(b'n' as i32) {
            if RE_STATE.with(|s| s.reg_string.get()) != 0 {
                ret = regnode(EXACTLY);
                regc(NL);
                regc(NUL as i32);
                *flagp |= HASWIDTH | SIMPLE;
            } else {
                ret = regnode(NEWL);
                *flagp |= HASWIDTH | HASNL;
            }
            return ret;
        } else if c == magic_fn(b'(' as i32) {
            if RE_STATE.with(|s| s.one_exactly.get()) {
                semsg!(
                    tr(E_INVALID_ITEM_IN_STR_BRACKETS),
                    if RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL {
                        c""
                    } else {
                        c"\\"
                    },
                );
                msg_state().rc_did_emsg.set(true);
                return None;
            }
            ret = reg(REG_PAREN, &mut flags);
            ret?;
            *flagp |= flags & (HASWIDTH | SPSTART | HASNL | HASLOOKBH);
            return ret;
        } else if c == NUL as i32
            || c == magic_fn(b'|' as i32)
            || c == magic_fn(b'&' as i32)
            || c == magic_fn(b')' as i32)
        {
            if RE_STATE.with(|s| s.one_exactly.get()) {
                semsg!(
                    tr(E_INVALID_ITEM_IN_STR_BRACKETS),
                    if RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL {
                        c""
                    } else {
                        c"\\"
                    },
                );
                msg_state().rc_did_emsg.set(true);
                return None;
            }
            iemsg(tr(E_INTERNAL_ERROR_IN_REGEXP));
            msg_state().rc_did_emsg.set(true);
            return None;
        } else if c == magic_fn(b'=' as i32)
            || c == magic_fn(b'?' as i32)
            || c == magic_fn(b'+' as i32)
            || c == magic_fn(b'@' as i32)
            || c == magic_fn(b'{' as i32)
            || c == magic_fn(b'*' as i32)
        {
            c = no_magic(c);
            let magic_cond = if c == b'*' as i32 {
                RE_STATE.with(|s| s.reg_magic.get()) >= MAGIC_ON
            } else {
                RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL
            };
            semsg!(
                tr(c"E64: %s%c follows nothing"),
                if magic_cond { c"" } else { c"\\" },
                c,
            );
            msg_state().rc_did_emsg.set(true);
            return None;
        } else if c == magic_fn(b'~' as i32) {
            if let Some(ref prev_sub) = reg_prev_sub_ref() {
                ret = regnode(EXACTLY);
                for &b in prev_sub.to_bytes() {
                    regc(b as i32);
                }
                regc(NUL as i32);
                if !prev_sub.to_bytes().is_empty() {
                    *flagp |= HASWIDTH;
                    if prev_sub.to_bytes().len() == 1 {
                        *flagp |= SIMPLE;
                    }
                }
                return ret;
            } else {
                emsg(tr(E_NOPRESUB));
                msg_state().rc_did_emsg.set(true);
                return None;
            }
        } else if (magic_fn(b'1' as i32)..=magic_fn(b'9' as i32)).contains(&c) {
            let refnum = c - magic_fn(b'0' as i32);
            if !seen_endbrace(refnum) {
                return None;
            }
            return regnode(BACKREF + refnum);
        } else if c == magic_fn(b'z' as i32) {
            c = no_magic(getchr());
            match c as u8 {
                b'(' => {
                    if (RE_STATE.with(|s| s.reg_do_extmatch.get()) & REX_SET) == 0 {
                        emsg(tr(E_Z_NOT_ALLOWED));
                        msg_state().rc_did_emsg.set(true);
                        return None;
                    }
                    if RE_STATE.with(|s| s.one_exactly.get()) {
                        semsg!(
                            tr(E_INVALID_ITEM_IN_STR_BRACKETS),
                            if RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL {
                                c""
                            } else {
                                c"\\"
                            },
                        );
                        msg_state().rc_did_emsg.set(true);
                        return None;
                    }
                    ret = reg(REG_ZPAREN, &mut flags);
                    ret?;
                    *flagp |= flags & (HASWIDTH | SPSTART | HASNL | HASLOOKBH);
                    RE_STATE.with(|s| s.re_has_z.set(REX_SET));
                    return ret;
                }
                b'1'..=b'9' => {
                    if (RE_STATE.with(|s| s.reg_do_extmatch.get()) & REX_USE) == 0 {
                        emsg(tr(E_Z1_NOT_ALLOWED));
                        msg_state().rc_did_emsg.set(true);
                        return None;
                    }
                    ret = regnode(ZREF + c - b'0' as i32);
                    RE_STATE.with(|s| s.re_has_z.set(REX_USE));
                    return ret;
                }
                b's' => {
                    ret = regnode(MOPEN);
                    if !re_mult_next(c"\\zs") {
                        return None;
                    }
                    return ret;
                }
                b'e' => {
                    ret = regnode(MCLOSE);
                    if !re_mult_next(c"\\ze") {
                        return None;
                    }
                    return ret;
                }
                _ => {
                    emsg(tr(c"E68: Invalid character after \\z"));
                    msg_state().rc_did_emsg.set(true);
                    return None;
                }
            }
        } else if c == magic_fn(b'%' as i32) {
            c = no_magic(getchr());
            match c as u8 {
                b'(' => {
                    if RE_STATE.with(|s| s.one_exactly.get()) {
                        semsg!(
                            tr(E_INVALID_ITEM_IN_STR_BRACKETS),
                            if RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL {
                                c""
                            } else {
                                c"\\"
                            },
                        );
                        msg_state().rc_did_emsg.set(true);
                        return None;
                    }
                    ret = reg(REG_NPAREN, &mut flags);
                    ret?;
                    *flagp |= flags & (HASWIDTH | SPSTART | HASNL | HASLOOKBH);
                    return ret;
                }
                b'^' => return regnode(RE_BOF),
                b'$' => return regnode(RE_EOF),
                b'#' => {
                    if re_st().regparse_byte() == b'='
                        && (b'0'..=b'2').contains(&re_st().regparse_byte_at(1))
                    {
                        semsg!(
                            tr(E_ATOM_ENGINE_MUST_BE_AT_START_OF_PATTERN),
                            i32::from(re_st().regparse_byte_at(1)),
                        );
                        return None;
                    }
                    return regnode(CURSOR);
                }
                b'V' => return regnode(RE_VISUAL),
                b'C' => return regnode(RE_COMPOSING),
                b'[' => {
                    if RE_STATE.with(|s| s.one_exactly.get()) {
                        semsg!(
                            tr(E_INVALID_ITEM_IN_STR_BRACKETS),
                            if RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL {
                                c""
                            } else {
                                c"\\"
                            },
                        );
                        msg_state().rc_did_emsg.set(true);
                        return None;
                    }
                    let mut lastnode: Option<usize> = None;
                    ret = None;
                    loop {
                        c = getchr();
                        if c == b']' as i32 {
                            break;
                        }
                        if c == NUL as i32 {
                            semsg!(
                                tr(E_MISSING_SB),
                                if RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL {
                                    c""
                                } else {
                                    c"\\"
                                },
                            );
                            msg_state().rc_did_emsg.set(true);
                            return None;
                        }
                        let br = regnode(BRANCH);
                        if ret.is_none() {
                            ret = br;
                        } else {
                            regtail(lastnode, br);
                            if RE_STATE.with(|s| s.reg_toolong.get()) != 0 {
                                return None;
                            }
                        }
                        ungetchr();
                        RE_STATE.with(|s| s.one_exactly.set(true));
                        lastnode = regatom(flagp);
                        RE_STATE.with(|s| s.one_exactly.set(false));
                        lastnode?;
                    }
                    if ret.is_none() {
                        semsg!(
                            tr(E_EMPTY_SB),
                            if RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL {
                                c""
                            } else {
                                c"\\"
                            },
                        );
                        msg_state().rc_did_emsg.set(true);
                        return None;
                    }
                    let lastbranch = regnode(BRANCH);
                    let mut br = regnode(NOTHING);
                    if ret != Some(JUST_CALC_SIZE) {
                        regtail(lastnode, br);
                        regtail(lastbranch, br);
                        br = ret;
                        while br != lastnode {
                            let br_idx = br.unwrap();
                            let is_branch =
                                bt_st(|st| op_fn(&st.reg_program.borrow(), br) == BRANCH);
                            if is_branch {
                                regtail(br, lastbranch);
                                if RE_STATE.with(|s| s.reg_toolong.get()) != 0 {
                                    return None;
                                }
                                br = Some(operand_idx(br_idx));
                            } else {
                                br = bt_st(|st| regnext_compiling(&st.reg_program.borrow(), br));
                            }
                        }
                    }
                    *flagp &= !(HASWIDTH | SIMPLE);
                    return ret;
                }
                b'd' | b'o' | b'x' | b'u' | b'U' => {
                    let i = match c as u8 {
                        b'd' => getdecchrs(),
                        b'o' => getoctchrs(),
                        b'x' => gethexchrs(2),
                        b'u' => gethexchrs(4),
                        b'U' => gethexchrs(8),
                        _ => -1,
                    };
                    if i < 0 || i > i64::from(i32::MAX) {
                        semsg!(
                            tr(c"E678: Invalid character after %s%%[dxouU]"),
                            if RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL {
                                c""
                            } else {
                                c"\\"
                            },
                        );
                        msg_state().rc_did_emsg.set(true);
                        return None;
                    }
                    if use_multibytecode(i as i32) {
                        ret = regnode(MULTIBYTECODE);
                    } else {
                        ret = regnode(EXACTLY);
                    }
                    if i == 0 {
                        regc(0x0a);
                    } else {
                        regmbc(i as i32);
                    }
                    regc(NUL as i32);
                    *flagp |= HASWIDTH;
                    return ret;
                }
                _ => {
                    if ascii_isdigit(c)
                        || c == b'<' as i32
                        || c == b'>' as i32
                        || c == b'\'' as i32
                        || c == b'.' as i32
                    {
                        let mut n: u32 = 0;
                        let cmp = c;
                        let mut cur = false;
                        let mut got_digit = false;

                        if cmp == b'<' as i32 || cmp == b'>' as i32 {
                            c = getchr();
                        }
                        if no_magic(c) == b'.' as i32 {
                            cur = true;
                            c = getchr();
                        }
                        while ascii_isdigit(c) {
                            got_digit = true;
                            n = n.wrapping_mul(10).wrapping_add((c - b'0' as i32) as u32);
                            c = getchr();
                        }
                        if no_magic(c) == b'\'' as i32 && n == 0 {
                            c = getchr();
                            ret = regnode(RE_MARK);
                            if ret == Some(JUST_CALC_SIZE) {
                                RE_STATE.with(|s| s.regsize.set(s.regsize.get() + (2)));
                            } else {
                                bt_st(|st| {
                                    let idx = st.regcode_idx.get();
                                    let mut prog = st.reg_program.borrow_mut();
                                    prog[idx] = c as u8;
                                    prog[idx + 1] = cmp as u8;
                                    st.regcode_idx.set(idx + 2);
                                });
                            }
                            return ret;
                        } else if (c == b'l' as i32 || c == b'c' as i32 || c == b'v' as i32)
                            && (cur || got_digit)
                        {
                            if cur && n != 0 {
                                semsg!(tr(E_REGEXP_NUMBER_AFTER_DOT_POS_SEARCH_CHR), no_magic(c),);
                                msg_state().rc_did_emsg.set(true);
                                return None;
                            }
                            if c == b'l' as i32 {
                                if cur {
                                    n = crate::globals::curwin().w_cursor.get().lnum as u32;
                                }
                                ret = regnode(RE_LNUM);
                                if save_prev_at_start != 0 {
                                    RE_STATE.with(|s| s.at_start.set(1));
                                }
                            } else if c == b'c' as i32 {
                                if cur {
                                    n = crate::globals::curwin().w_cursor.get().col as u32 + 1;
                                }
                                ret = regnode(RE_COL);
                            } else {
                                if cur {
                                    let mut vcol: Colnr = 0;
                                    let mut cursor = crate::globals::curwin().w_cursor.get();
                                    getvvcol(
                                        &crate::globals::curwin(),
                                        &mut cursor,
                                        None,
                                        None,
                                        Some(&mut vcol),
                                        0,
                                    );
                                    crate::globals::curwin().w_cursor.set(cursor);
                                    vcol += 1;
                                    n = vcol as u32;
                                }
                                ret = regnode(RE_VCOL);
                            }
                            if ret == Some(JUST_CALC_SIZE) {
                                RE_STATE.with(|s| s.regsize.set(s.regsize.get() + (5)));
                            } else {
                                bt_st(|st| {
                                    let mut prog = st.reg_program.borrow_mut();
                                    let idx = re_put_uint32(&mut prog, st.regcode_idx.get(), n);
                                    prog[idx] = cmp as u8;
                                    st.regcode_idx.set(idx + 1);
                                });
                            }
                            return ret;
                        }
                    }
                    semsg!(
                        tr(c"E71: Invalid character after %s%%"),
                        if RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL {
                            c""
                        } else {
                            c"\\"
                        },
                    );
                    msg_state().rc_did_emsg.set(true);
                    return None;
                }
            }
        } else if c == magic_fn(b'[' as i32) {
            goto_collection = true;
        }
    }

    if goto_collection {
        let (len, is_closing, char_class) = re_st().with_regparse_bytes(|start_bytes| {
            let rem_bytes = skip_anyof_slice(start_bytes);
            let len = start_bytes.len() - rem_bytes.len();
            let is_closing = rem_bytes.first() == Some(&b']');
            let char_class = if is_closing && extra == 0 {
                bt_recognize_char_class(&start_bytes[..len + 1])
            } else {
                0
            };
            (len, is_closing, char_class)
        });
        let lp_off = RE_STATE.with(|s| s.regparse_off.get()) + len;
        if is_closing {
            let mut startc: i32 = -1;
            if extra == 0 && char_class != 0 {
                ret = regnode(char_class);
                RE_STATE.with(|s| s.regparse_off.set(lp_off));
                skipchr();
                *flagp |= HASWIDTH | SIMPLE;
                return ret;
            }

            if re_st().regparse_byte() == b'^' {
                ret = regnode(ANYBUT + extra);
                RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() + (1)));
            } else {
                ret = regnode(ANYOF + extra);
            }

            if re_st().regparse_byte() == b']' || re_st().regparse_byte() == b'-' {
                startc = i32::from(re_st().regparse_byte());
                regc(startc);
                RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() + (1)));
            }

            while re_st().regparse_byte() != 0 && re_st().regparse_byte() != b']' {
                if re_st().regparse_byte() == b'-' {
                    RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() + (1)));
                    if re_st().regparse_byte() == b']'
                        || re_st().regparse_byte() == 0
                        || startc == -1
                        || (re_st().regparse_byte() == b'\\' && re_st().regparse_byte_at(1) == b'n')
                    {
                        regc(b'-' as i32);
                        startc = b'-' as i32;
                    } else {
                        let mut endc: i32 = 0;
                        if re_st().regparse_byte() == b'[' {
                            endc = re_st().regparse_get_coll_element();
                        }
                        if endc == 0 {
                            let adv = re_st().with_regparse_bytes(|start_bytes| {
                                let mut rem = start_bytes;
                                endc = mb_ptr2char_adv(&mut rem);
                                start_bytes.len() - rem.len()
                            });
                            RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() + (adv)));
                        }
                        if endc == b'\\' as i32 && RE_STATE.with(|s| s.reg_cpo_lit.get()) == 0 {
                            endc = coll_get_char();
                        }
                        if startc > endc {
                            emsg(tr(E_REVERSE_RANGE));
                            msg_state().rc_did_emsg.set(true);
                            return None;
                        }
                        if utf_char2len(startc) > 1 || utf_char2len(endc) > 1 {
                            if endc > startc + 256 {
                                emsg(tr(E_LARGE_CLASS));
                                msg_state().rc_did_emsg.set(true);
                                return None;
                            }
                            while {
                                startc += 1;
                                startc <= endc
                            } {
                                regmbc(startc);
                            }
                        } else {
                            while {
                                startc += 1;
                                startc <= endc
                            } {
                                regc(startc);
                            }
                        }
                        startc = -1;
                    }
                } else if re_st().regparse_byte() == b'\\'
                    && (crate::strings::vim_haschr(
                        REGEXP_INRANGE,
                        i32::from(re_st().regparse_byte_at(1)),
                    ) || (RE_STATE.with(|s| s.reg_cpo_lit.get()) == 0
                        && crate::strings::vim_haschr(
                            REGEXP_ABBR,
                            i32::from(re_st().regparse_byte_at(1)),
                        )))
                {
                    RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() + (1)));
                    if re_st().regparse_byte() == b'n' {
                        if let Some(ret_idx) = ret
                            && ret_idx != JUST_CALC_SIZE
                            && bt_st(|st| st.reg_program.borrow()[ret_idx] == ANYOF as u8)
                        {
                            bt_st(|st| {
                                st.reg_program.borrow_mut()[ret_idx] = (ANYOF + ADD_NL) as u8
                            });
                            *flagp |= HASNL;
                        }
                        RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() + (1)));
                        startc = -1;
                    } else if matches!(re_st().regparse_byte(), b'd' | b'o' | b'x' | b'u' | b'U') {
                        startc = coll_get_char();
                        if startc == i32::MAX {
                            emsg(tr(E_UNICODE_VAL_TOO_LARGE));
                            msg_state().rc_did_emsg.set(true);
                            return None;
                        }
                        if startc == 0 {
                            regc(0x0a);
                        } else {
                            regmbc(startc);
                        }
                    } else {
                        startc = backslash_trans(i32::from(re_st().regparse_byte()));
                        RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() + (1)));
                        regc(startc);
                    }
                } else if re_st().regparse_byte() == b'[' {
                    let mut c_class = re_st().regparse_get_char_class();
                    startc = -1;
                    match c_class {
                        CLASS_NONE => {
                            c_class = re_st().regparse_get_equi_class();
                            if c_class != 0 {
                                reg_equi_class(c_class);
                            } else {
                                c_class = re_st().regparse_get_coll_element();
                                if c_class != 0 {
                                    regmbc(c_class);
                                } else {
                                    startc = i32::from(re_st().regparse_byte());
                                    RE_STATE
                                        .with(|s| s.regparse_off.set(s.regparse_off.get() + (1)));
                                    regc(startc);
                                }
                            }
                        }
                        CLASS_ALNUM => {
                            for cu in 1..128u8 {
                                if cu.is_ascii_alphanumeric() {
                                    regmbc(cu as i32);
                                }
                            }
                        }
                        CLASS_ALPHA => {
                            for cu in 1..128u8 {
                                if cu.is_ascii_alphabetic() {
                                    regmbc(cu as i32);
                                }
                            }
                        }
                        CLASS_BLANK => {
                            regc(b' ' as i32);
                            regc(b'\t' as i32);
                        }
                        CLASS_CNTRL => {
                            for cu in 1..=127u8 {
                                if cu.is_ascii_control() {
                                    regmbc(cu as i32);
                                }
                            }
                        }
                        CLASS_DIGIT => {
                            for cu in 1..=127u8 {
                                if ascii_isdigit(cu as i32) {
                                    regmbc(cu as i32);
                                }
                            }
                        }
                        CLASS_GRAPH => {
                            for cu in 1..=127u8 {
                                if cu.is_ascii_graphic() {
                                    regmbc(cu as i32);
                                }
                            }
                        }
                        CLASS_LOWER => {
                            for cu in 1..=255 {
                                if mb_islower(cu) && cu != 170 && cu != 186 {
                                    regmbc(cu);
                                }
                            }
                        }
                        CLASS_PRINT => {
                            for cu in 1..=255 {
                                if vim_isprintc(cu) {
                                    regmbc(cu);
                                }
                            }
                        }
                        CLASS_PUNCT => {
                            for cu in 1..128u8 {
                                if cu.is_ascii_punctuation() {
                                    regmbc(cu as i32);
                                }
                            }
                        }
                        CLASS_SPACE => {
                            for cu in 9..=13 {
                                regc(cu);
                            }
                            regc(b' ' as i32);
                        }
                        CLASS_UPPER => {
                            for cu in 1..=255 {
                                if mb_isupper(cu) {
                                    regmbc(cu);
                                }
                            }
                        }
                        CLASS_XDIGIT => {
                            for cu in 1..=255 {
                                if ascii_isxdigit(cu) {
                                    regmbc(cu);
                                }
                            }
                        }
                        CLASS_TAB => regc(b'\t' as i32),
                        CLASS_RETURN => regc(b'\r' as i32),
                        CLASS_BACKSPACE => regc(BS),
                        CLASS_ESCAPE => regc(ESC),
                        CLASS_IDENT => {
                            for cu in 1..=255 {
                                if vim_is_idc(cu) {
                                    regmbc(cu);
                                }
                            }
                        }
                        CLASS_KEYWORD => {
                            let chartab = curbuf_chartab();
                            for cu in 1..=255 {
                                if crate::charset::vim_iswordc_tab(cu, &chartab) {
                                    regmbc(cu);
                                }
                            }
                        }
                        CLASS_FNAME => {
                            for cu in 1..=255 {
                                if vim_isfilec(cu) {
                                    regmbc(cu);
                                }
                            }
                        }
                        _ => {}
                    }
                } else {
                    startc = re_st().with_regparse_bytes(|b| utf_ptr2char(b));
                    let mut len = re_st().with_regparse_bytes(|b| utfc_ptr2len(b));
                    if utf_char2len(startc) != len {
                        startc = -1;
                    }
                    while {
                        len -= 1;
                        len >= 0
                    } {
                        regc(i32::from(re_st().regparse_byte()));
                        RE_STATE.with(|s| s.regparse_off.set(s.regparse_off.get() + (1)));
                    }
                }
            }
            regc(NUL as i32);
            RE_STATE.with(|s| s.prevchr_len.set(1));
            if re_st().regparse_byte() != b']' {
                emsg(tr(E_TOOMSBRA));
                msg_state().rc_did_emsg.set(true);
                return None;
            }
            skipchr();
            *flagp |= HASWIDTH | SIMPLE;
            return ret;
        } else if RE_STATE.with(|s| s.reg_strict.get()) != 0 {
            semsg!(
                tr(E_MISSINGBRACKET),
                if RE_STATE.with(|s| s.reg_magic.get()) > MAGIC_OFF {
                    c""
                } else {
                    c"\\"
                },
            );
            msg_state().rc_did_emsg.set(true);
            return None;
        }
    }

    if goto_multibyte || use_multibytecode(c) {
        ret = regnode(MULTIBYTECODE);
        regmbc(c);
        *flagp |= HASWIDTH | SIMPLE;
        return ret;
    }

    ret = regnode(EXACTLY);
    let mut len: i32 = 0;
    while c != NUL as i32
        && (len == 0
            || (re_multi_type(peekchr()) == NOT_MULTI
                && !RE_STATE.with(|s| s.one_exactly.get())
                && !is_magic_fn(c)))
    {
        c = no_magic(c);
        regmbc(c);
        let mut state: GraphemeState = GRAPHEME_STATE_INIT;
        loop {
            let next_ch = re_st().with_regparse_bytes(|p_bytes| {
                let l = utf_ptr2len(p_bytes) as usize;
                if !utf_composinglike(p_bytes, p_bytes.get(l..).unwrap_or(&[]), Some(&mut state)) {
                    None
                } else {
                    Some(utf_ptr2char(p_bytes))
                }
            });
            let Some(ch) = next_ch else {
                break;
            };
            regmbc(ch);
            skipchr();
        }
        c = getchr();
        len += 1;
    }
    ungetchr();

    regc(NUL as i32);
    *flagp |= HASWIDTH;
    if len == 1 {
        *flagp |= SIMPLE;
    }
    ret
}

fn regpiece(flagp: &mut i32) -> Option<usize> {
    let mut flags: i32 = 0;
    let mut minval: i32 = 0;
    let mut maxval: i32 = 0;

    let ret = regatom(&mut flags)?;

    let op = peekchr();
    if re_multi_type(op) == NOT_MULTI {
        *flagp = flags;
        return Some(ret);
    }
    *flagp = WORST | SPSTART | (flags & (HASNL | HASLOOKBH));

    skipchr();
    if op == magic_fn(b'*' as i32) {
        if (flags & SIMPLE) != 0 {
            reginsert(STAR, Some(ret));
        } else {
            reginsert(BRANCH, Some(ret));
            regoptail(Some(ret), regnode(BACK));
            regoptail(Some(ret), Some(ret));
            regtail(Some(ret), regnode(BRANCH));
            regtail(Some(ret), regnode(NOTHING));
        }
    } else if op == magic_fn(b'+' as i32) {
        if (flags & SIMPLE) != 0 {
            reginsert(PLUS, Some(ret));
        } else {
            let next = regnode(BRANCH);
            regtail(Some(ret), next);
            regtail(regnode(BACK), Some(ret));
            regtail(next, regnode(BRANCH));
            regtail(Some(ret), regnode(NOTHING));
        }
        *flagp = WORST | HASWIDTH | (flags & (HASNL | HASLOOKBH));
    } else if op == magic_fn(b'@' as i32) {
        let mut lop = END;
        let mut nr = getdecchrs();
        match no_magic(getchr()) as u8 {
            b'=' => lop = MATCH,
            b'!' => lop = NOMATCH,
            b'>' => lop = SUBPAT,
            b'<' => match no_magic(getchr()) as u8 {
                b'=' => lop = BEHIND,
                b'!' => lop = NOBEHIND,
                _ => {}
            },
            _ => {}
        }
        if lop == END {
            semsg!(
                tr(E_INVALID_CHARACTER_AFTER_STR_AT),
                if RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL {
                    c""
                } else {
                    c"\\"
                },
            );
            msg_state().rc_did_emsg.set(true);
            return None;
        }
        if lop == BEHIND || lop == NOBEHIND {
            regtail(Some(ret), regnode(BHPOS));
            *flagp |= HASLOOKBH;
        }
        regtail(Some(ret), regnode(END));
        if lop == BEHIND || lop == NOBEHIND {
            if nr < 0 {
                nr = 0;
            }
            reginsert_nr(lop, i64::from(nr as u32), Some(ret));
        } else {
            reginsert(lop, Some(ret));
        }
    } else if op == magic_fn(b'?' as i32) || op == magic_fn(b'=' as i32) {
        reginsert(BRANCH, Some(ret));
        regtail(Some(ret), regnode(BRANCH));
        let next = regnode(NOTHING);
        regtail(Some(ret), next);
        regoptail(Some(ret), next);
    } else if op == magic_fn(b'{' as i32) {
        if !read_limits(&mut minval, &mut maxval) {
            return None;
        }
        if (flags & SIMPLE) != 0 {
            reginsert(BRACE_SIMPLE, Some(ret));
            reginsert_limits(
                BRACE_LIMITS,
                i64::from(minval),
                i64::from(maxval),
                Some(ret),
            );
        } else {
            if RE_STATE.with(|s| s.num_complex_braces.get()) >= 10 {
                semsg!(
                    tr(c"E60: Too many complex %s{...}s"),
                    if RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL {
                        c""
                    } else {
                        c"\\"
                    },
                );
                msg_state().rc_did_emsg.set(true);
                return None;
            }
            reginsert(
                BRACE_COMPLEX + RE_STATE.with(|s| s.num_complex_braces.get()),
                Some(ret),
            );
            regoptail(Some(ret), regnode(BACK));
            regoptail(Some(ret), Some(ret));
            reginsert_limits(
                BRACE_LIMITS,
                i64::from(minval),
                i64::from(maxval),
                Some(ret),
            );
            RE_STATE.with(|s| s.num_complex_braces.set(s.num_complex_braces.get() + (1)));
        }
        if minval > 0 && maxval > 0 {
            *flagp = HASWIDTH | (flags & (HASNL | HASLOOKBH));
        }
    }

    if re_multi_type(peekchr()) != NOT_MULTI {
        if peekchr() == magic_fn(b'*' as i32) {
            semsg!(
                tr(c"E61: Nested %s*"),
                if RE_STATE.with(|s| s.reg_magic.get()) >= MAGIC_ON {
                    c""
                } else {
                    c"\\"
                },
            );
            msg_state().rc_did_emsg.set(true);
            return None;
        }
        semsg!(
            tr(c"E62: Nested %s%c"),
            if RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL {
                c""
            } else {
                c"\\"
            },
            no_magic(peekchr()),
        );
        msg_state().rc_did_emsg.set(true);
        return None;
    }

    Some(ret)
}

fn regconcat(flagp: &mut i32) -> Option<usize> {
    let mut first: Option<usize> = None;
    let mut chain: Option<usize> = None;
    let mut flags: i32 = 0;
    let mut cont = true;

    *flagp = WORST;

    while cont {
        let p = peekchr();
        if p == NUL as i32
            || p == magic_fn(b'|' as i32)
            || p == magic_fn(b'&' as i32)
            || p == magic_fn(b')' as i32)
        {
            cont = false;
        } else if p == magic_fn(b'Z' as i32) {
            RE_STATE.with(|s| s.regflags.set(s.regflags.get() | (RF_ICOMBINE)));
            skipchr_keepstart();
        } else if p == magic_fn(b'c' as i32) {
            RE_STATE.with(|s| s.regflags.set(s.regflags.get() | (RF_ICASE)));
            skipchr_keepstart();
        } else if p == magic_fn(b'C' as i32) {
            RE_STATE.with(|s| s.regflags.set(s.regflags.get() | (RF_NOICASE)));
            skipchr_keepstart();
        } else if p == magic_fn(b'v' as i32) {
            RE_STATE.with(|s| s.reg_magic.set(MAGIC_ALL));
            skipchr_keepstart();
            RE_STATE.with(|s| s.curchr.set(-1));
        } else if p == magic_fn(b'm' as i32) {
            RE_STATE.with(|s| s.reg_magic.set(MAGIC_ON));
            skipchr_keepstart();
            RE_STATE.with(|s| s.curchr.set(-1));
        } else if p == magic_fn(b'M' as i32) {
            RE_STATE.with(|s| s.reg_magic.set(MAGIC_OFF));
            skipchr_keepstart();
            RE_STATE.with(|s| s.curchr.set(-1));
        } else if p == magic_fn(b'V' as i32) {
            RE_STATE.with(|s| s.reg_magic.set(MAGIC_NONE));
            skipchr_keepstart();
            RE_STATE.with(|s| s.curchr.set(-1));
        } else {
            let latest = regpiece(&mut flags);
            if latest.is_none() || RE_STATE.with(|s| s.reg_toolong.get()) != 0 {
                return None;
            }
            *flagp |= flags & (HASWIDTH | HASNL | HASLOOKBH);
            if chain.is_none() {
                *flagp |= flags & SPSTART;
            } else {
                regtail(chain, latest);
            }
            chain = latest;
            if first.is_none() {
                first = latest;
            }
        }
    }
    if first.is_none() {
        first = regnode(NOTHING);
    }
    first
}

fn regbranch(flagp: &mut i32) -> Option<usize> {
    let mut chain: Option<usize> = None;
    let mut flags: i32 = 0;

    *flagp = WORST | HASNL;

    let ret = regnode(BRANCH);
    loop {
        let latest = regconcat(&mut flags)?;
        *flagp |= flags & (HASWIDTH | SPSTART | HASLOOKBH);
        *flagp &= !HASNL | (flags & HASNL);
        if chain.is_some() {
            regtail(chain, Some(latest));
        }
        if peekchr() != magic_fn(b'&' as i32) {
            break;
        }
        skipchr();
        regtail(Some(latest), regnode(END));
        if RE_STATE.with(|s| s.reg_toolong.get()) != 0 {
            break;
        }
        reginsert(MATCH, Some(latest));
        chain = Some(latest);
    }
    ret
}

fn reg(paren: i32, flagp: &mut i32) -> Option<usize> {
    let mut ret: Option<usize>;
    let mut parno: i32 = 0;
    let mut flags: i32 = 0;

    *flagp = HASWIDTH;

    if paren == REG_ZPAREN {
        if RE_STATE.with(|s| s.regnzpar.get()) >= NSUBEXP as i32 {
            emsg(tr(c"E50: Too many \\z("));
            msg_state().rc_did_emsg.set(true);
            return None;
        }
        parno = RE_STATE.with(|s| s.regnzpar.get());
        RE_STATE.with(|s| s.regnzpar.set(s.regnzpar.get() + (1)));
        ret = regnode(ZOPEN + parno);
    } else if paren == REG_PAREN {
        if RE_STATE.with(|s| s.regnpar.get()) >= NSUBEXP as i32 {
            semsg!(
                tr(c"E51: Too many %s("),
                if RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL {
                    c""
                } else {
                    c"\\"
                },
            );
            msg_state().rc_did_emsg.set(true);
            return None;
        }
        parno = RE_STATE.with(|s| s.regnpar.get());
        RE_STATE.with(|s| s.regnpar.set(s.regnpar.get() + (1)));
        ret = regnode(MOPEN + parno);
    } else if paren == REG_NPAREN {
        ret = regnode(NOPEN);
    } else {
        ret = None;
    }

    if RE_STATE.with(|s| s.bt_reg_parse_depth.get()) >= REG_MAX_PAREN_DEPTH {
        emsg(tr(E_COMMAND_TOO_COMPLEX));
        msg_state().rc_did_emsg.set(true);
        return None;
    }
    RE_STATE.with(|s| s.bt_reg_parse_depth.set(s.bt_reg_parse_depth.get() + (1)));

    let mut br = regbranch(&mut flags);
    if br.is_none() {
        RE_STATE.with(|s| s.bt_reg_parse_depth.set(s.bt_reg_parse_depth.get() - (1)));
        return None;
    }
    if ret.is_some() {
        regtail(ret, br);
    } else {
        ret = br;
    }
    if (flags & HASWIDTH) == 0 {
        *flagp &= !HASWIDTH;
    }
    *flagp |= flags & (SPSTART | HASNL | HASLOOKBH);
    while peekchr() == magic_fn(b'|' as i32) {
        skipchr();
        br = regbranch(&mut flags);
        if br.is_none() || RE_STATE.with(|s| s.reg_toolong.get()) != 0 {
            RE_STATE.with(|s| s.bt_reg_parse_depth.set(s.bt_reg_parse_depth.get() - (1)));
            return None;
        }
        regtail(ret, br);
        if (flags & HASWIDTH) == 0 {
            *flagp &= !HASWIDTH;
        }
        *flagp |= flags & (SPSTART | HASNL | HASLOOKBH);
    }

    let ender = regnode(if paren == REG_ZPAREN {
        ZCLOSE + parno
    } else if paren == REG_PAREN {
        MCLOSE + parno
    } else if paren == REG_NPAREN {
        NCLOSE
    } else {
        END
    });
    regtail(ret, ender);

    br = ret;
    while br.is_some() {
        regoptail(br, ender);
        br = bt_st(|st| regnext_compiling(&st.reg_program.borrow(), br));
    }

    if paren != REG_NOPAREN && getchr() != magic_fn(b')' as i32) {
        if paren == REG_ZPAREN {
            emsg(tr(E_UNMATCHED_Z));
        } else if paren == REG_NPAREN {
            semsg!(
                tr(E_UNMATCHED_STR_PERCENT_OPEN),
                if RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL {
                    c""
                } else {
                    c"\\"
                },
            );
        } else {
            semsg!(
                tr(E_UNMATCHED_STR_OPEN),
                if RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL {
                    c""
                } else {
                    c"\\"
                },
            );
        }
        msg_state().rc_did_emsg.set(true);
        RE_STATE.with(|s| s.bt_reg_parse_depth.set(s.bt_reg_parse_depth.get() - (1)));
        return None;
    } else if paren == REG_NOPAREN && peekchr() != NUL as i32 {
        if RE_STATE.with(|s| s.curchr.get()) == magic_fn(b')' as i32) {
            semsg!(
                tr(E_UNMATCHED_STR_CLOSE),
                if RE_STATE.with(|s| s.reg_magic.get()) == MAGIC_ALL {
                    c""
                } else {
                    c"\\"
                },
            );
        } else {
            emsg(tr(E_TRAILING));
        }
        msg_state().rc_did_emsg.set(true);
        RE_STATE.with(|s| s.bt_reg_parse_depth.set(s.bt_reg_parse_depth.get() - (1)));
        return None;
    }

    if paren == REG_PAREN {
        RE_STATE.with(|s| s.had_endbrace[parno as usize].set(1));
    }

    RE_STATE.with(|s| s.bt_reg_parse_depth.set(s.bt_reg_parse_depth.get() - (1)));
    ret
}

pub(super) fn bt_regcomp(expr: &[u8], re_flags: i32) -> Option<BtRegprog> {
    let mut flags: i32 = 0;

    regcomp_start(expr, re_flags);
    bt_st(|st| st.regcode_idx.set(JUST_CALC_SIZE));
    regc(REGMAGIC);
    reg(REG_NOPAREN, &mut flags)?;

    let regsize = RE_STATE.with(|s| s.regsize.get()) as usize;
    bt_st(|st| {
        *st.reg_program.borrow_mut() = vec![0u8; regsize];
        st.regcode_idx.set(0);
    });
    regcomp_start(expr, re_flags);
    regc(REGMAGIC);
    if reg(REG_NOPAREN, &mut flags).is_none() || RE_STATE.with(|s| s.reg_toolong.get()) != 0 {
        bt_st(|st| st.reg_program.borrow_mut().clear());
        if RE_STATE.with(|s| s.reg_toolong.get()) != 0 {
            emsg(tr(c"E339: Pattern too long"));
            msg_state().rc_did_emsg.set(true);
        }
        return None;
    }

    let mut r = BtRegprog {
        regflags: RE_STATE.with(|s| s.regflags.get()),
        regstart: NUL as i32,
        regstart_class: 0,
        reganch: 0,
        regmust_off: None,
        regmlen: 0,
        reghasz: RE_STATE.with(|s| s.re_has_z.get()) as u8,
        program: bt_st(|st| std::mem::take(&mut *st.reg_program.borrow_mut())),
    };
    debug_assert!(
        r.program.len() >= 3 && op_fn(&r.program, Some(r.program.len() - 3)) == END,
        "the pattern's END is the last node"
    );
    if (flags & HASNL) != 0 {
        r.regflags |= RF_HASNL;
    }
    if (flags & HASLOOKBH) != 0 {
        r.regflags |= RF_LOOKBH;
    }

    let mut scan = Some(1usize);
    if op_fn(&r.program, regnext(&r.program, scan)) == END {
        scan = Some(operand_idx(1));

        if op_fn(&r.program, scan) == BOL || op_fn(&r.program, scan) == RE_BOF {
            r.reganch += 1;
            scan = regnext(&r.program, scan);
        }

        if let Some(s_idx) = scan {
            let first_op = op_fn(&r.program, scan);
            if first_op == EXACTLY {
                r.regstart = crate::mbyte::utf_slice2char(&r.program[operand_idx(s_idx)..]);
            } else if matches!(
                first_op,
                BOW | EOW | NOTHING | MOPEN | NOPEN | MCLOSE | NCLOSE
            ) {
                let regnext_scan = regnext(&r.program, scan);
                if let Some(rn_idx) = regnext_scan {
                    let next_op = op_fn(&r.program, regnext_scan);
                    if next_op == EXACTLY {
                        r.regstart =
                            crate::mbyte::utf_slice2char(&r.program[operand_idx(rn_idx)..]);
                    } else {
                        let class_op = if next_op == PLUS {
                            op_fn(&r.program, Some(operand_idx(rn_idx)))
                        } else {
                            next_op
                        };
                        if let Some((mask, tv)) = class_mask_testval(class_op)
                            && tv != 0
                        {
                            r.regstart_class = mask;
                        }
                    }
                }
            } else {
                let class_op = if first_op == PLUS {
                    op_fn(&r.program, Some(operand_idx(s_idx)))
                } else {
                    first_op
                };
                if let Some((mask, tv)) = class_mask_testval(class_op)
                    && tv != 0
                {
                    r.regstart_class = mask;
                }
            }
        }

        if ((flags & SPSTART) != 0
            || op_fn(&r.program, scan) == BOW
            || op_fn(&r.program, scan) == EOW)
            && (flags & HASNL) == 0
        {
            let mut longest: Option<usize> = None;
            let mut len: i32 = 0;
            while let Some(s_idx) = scan {
                if op_fn(&r.program, scan) == EXACTLY {
                    let opnd = operand_idx(s_idx);
                    let scanlen = crate::memory::bytes_until_nul(&r.program[opnd..]).len();
                    if scanlen >= len as usize {
                        longest = Some(opnd);
                        len = scanlen as i32;
                    }
                }
                scan = regnext(&r.program, scan);
            }
            r.regmust_off = longest;
            r.regmlen = len;
        }
    }
    Some(r)
}

#[inline(always)]
fn reg_save(rex: &Regexec<'_>, backpos_len: usize, save: &mut Regsave) {
    if rex.reg_multi() {
        save.rs_u = RegPos::from_pos(rex.cur_pos());
    } else {
        save.rs_u.ptr = Some(rex.input);
    }
    save.rs_len = backpos_len as i32;
}

/// Restores the position `save` holds.
#[inline(always)]
fn reg_restore(rex: &mut Regexec<'_>, backpos: &mut Vec<Backpos>, save: &Regsave) {
    if rex.reg_multi() {
        let pos = save.rs_u.pos();
        if rex.lnum != pos.lnum {
            rex.lnum = pos.lnum;
            rex.line = rex.getline(pos.lnum).unwrap_or_default();
        }
        rex.input = pos.col as usize;
    } else if let Some(ptr) = save.rs_u.ptr {
        rex.input = ptr;
    }
    backpos.truncate(save.rs_len as usize);
}

/// Whether the current position is the one `save` holds.
#[inline(always)]
fn reg_save_equal(rex: &Regexec<'_>, save: &Regsave) -> bool {
    if rex.reg_multi() {
        let pos = save.rs_u.pos();
        rex.lnum == pos.lnum && rex.input_col() == pos.col
    } else {
        save.rs_u.ptr == Some(rex.input)
    }
}

/// Saves `*posp` in `savep` and sets it to the current position `cur`.
#[inline(always)]
fn save_se_multi(savep: &mut SaveSe, posp: &mut Lpos, cur: Lpos) {
    savep.se_u = RegPos::from_pos(*posp);
    *posp = cur;
}

/// Saves `*pp` in `savep` and sets it to the current position `input`.
#[inline(always)]
fn save_se_one(savep: &mut SaveSe, pp: &mut Option<usize>, input: usize) {
    savep.se_u.ptr = *pp;
    *pp = Some(input);
}

#[inline(always)]
fn save_se_z(rex: &mut Regexec<'_>, savep: &mut SaveSe, no: usize, is_end: bool) {
    if rex.reg_multi() {
        let cur = rex.cur_pos();
        let posp = if is_end {
            &mut rex.reg_endzpos[no]
        } else {
            &mut rex.reg_startzpos[no]
        };
        save_se_multi(savep, posp, cur);
    } else {
        let input = rex.input;
        let pp = if is_end {
            &mut rex.reg_endzp[no]
        } else {
            &mut rex.reg_startzp[no]
        };
        save_se_one(savep, pp, input);
    }
}

#[inline(always)]
fn restore_se_z(rex: &mut Regexec<'_>, savep: &SaveSe, no: usize, is_end: bool) {
    if rex.reg_multi() {
        let posp = if is_end {
            &mut rex.reg_endzpos[no]
        } else {
            &mut rex.reg_startzpos[no]
        };
        *posp = savep.se_u.pos();
    } else {
        let pp = if is_end {
            &mut rex.reg_endzp[no]
        } else {
            &mut rex.reg_startzp[no]
        };
        *pp = savep.se_u.ptr;
    }
}

#[inline(always)]
fn save_se_m(rex: &mut Regexec<'_>, savep: &mut SaveSe, no: usize, is_end: bool) {
    if rex.reg_multi() {
        let cur = rex.cur_pos();
        let posp = if is_end {
            &mut rex.rmm_endpos[no]
        } else {
            &mut rex.rmm_startpos[no]
        };
        save_se_multi(savep, posp, cur);
    } else {
        let input = rex.input;
        let pp = if is_end {
            &mut rex.rm_endp[no]
        } else {
            &mut rex.rm_startp[no]
        };
        save_se_one(savep, pp, input);
    }
}

#[inline(always)]
fn restore_se_m(rex: &mut Regexec<'_>, savep: &SaveSe, no: usize, is_end: bool) {
    if rex.reg_multi() {
        let posp = if is_end {
            &mut rex.rmm_endpos[no]
        } else {
            &mut rex.rmm_startpos[no]
        };
        *posp = savep.se_u.pos();
    } else {
        let pp = if is_end {
            &mut rex.rm_endp[no]
        } else {
            &mut rex.rm_startp[no]
        };
        *pp = savep.se_u.ptr;
    }
}
#[inline(always)]
const fn class_mask_testval(base_op: i32) -> Option<(i16, i16)> {
    match base_op {
        WHITE => Some((RI_WHITE, RI_WHITE)),
        NWHITE => Some((RI_WHITE, 0)),
        DIGIT => Some((RI_DIGIT, RI_DIGIT)),
        NDIGIT => Some((RI_DIGIT, 0)),
        HEX => Some((RI_HEX, RI_HEX)),
        NHEX => Some((RI_HEX, 0)),
        OCTAL => Some((RI_OCTAL, RI_OCTAL)),
        NOCTAL => Some((RI_OCTAL, 0)),
        WORD => Some((RI_WORD, RI_WORD)),
        NWORD => Some((RI_WORD, 0)),
        HEAD => Some((RI_HEAD, RI_HEAD)),
        NHEAD => Some((RI_HEAD, 0)),
        ALPHA => Some((RI_ALPHA, RI_ALPHA)),
        NALPHA => Some((RI_ALPHA, 0)),
        LOWER => Some((RI_LOWER, RI_LOWER)),
        NLOWER => Some((RI_LOWER, 0)),
        UPPER => Some((RI_UPPER, RI_UPPER)),
        NUPPER => Some((RI_UPPER, 0)),
        _ => None,
    }
}

/// Whether a repeat of `op` at the end of the current line continues in the next line.
#[inline(always)]
fn repeat_crosses_line(rex: &Regexec<'_>, op: i32) -> bool {
    rex.reg_multi() && with_nl(op) && rex.lnum <= rex.reg_maxline && !rex.reg_line_lbr
}

/// Matches the simple item at `p` at most `maxcount` times from the current position, and
/// moves the position past what it matched; returns how many times it matched.
#[inline(always)]
fn regrepeat(
    rex: &mut Regexec<'_>,
    gst: &crate::input::InputGlobalState,
    prog: &[u8],
    p: usize,
    maxcount: i64,
) -> i32 {
    let mut count: i64 = 0;
    let mut scan = rex.input;
    let op = op_fn(prog, Some(p));
    let base_op = if with_nl(op) { op - ADD_NL } else { op };
    let opnd = &prog[operand_idx(p)..];

    if let Some((mask, testval)) = class_mask_testval(base_op) {
        while count < maxcount {
            let rest = rex.line_from(scan);
            let Some(&b0) = rest.first() else {
                if !repeat_crosses_line(rex, op) {
                    break;
                }
                rex.nextline();
                scan = rex.input;
                if gst.got_int.get() {
                    break;
                }
                count += 1;
                continue;
            };
            let l = utfc_ptr2len(rest) as usize;
            if l > 1 {
                if testval != 0 {
                    break;
                }
                scan += l;
            } else if (RI_CLASS_TAB[b0 as usize] & mask) == testval
                || (rex.reg_line_lbr && b0 == b'\n' && with_nl(op))
            {
                scan += 1;
            } else {
                break;
            }
            count += 1;
        }
        rex.input = scan;
        return count as i32;
    }

    match base_op {
        ANY => {
            while count < maxcount {
                // Anything matches up to the end of the line (or the end of the buffer for
                // ANY + ADD_NL), only limited by maxcount.
                let line = rex.line();
                while scan < line.len() && count < maxcount {
                    count += 1;
                    scan += utfc_ptr2len(&line[scan..]) as usize;
                }
                if !repeat_crosses_line(rex, op) || count == maxcount {
                    break;
                }
                count += 1;
                rex.nextline();
                scan = rex.input;
                if gst.got_int.get() {
                    break;
                }
            }
        }
        IDENT | SIDENT | KWORD | SKWORD | FNAME | SFNAME => {
            let testval = matches!(base_op, IDENT | KWORD | FNAME);
            while count < maxcount {
                let rest = rex.line_from(scan);
                let b0 = rest.first().copied().unwrap_or(NUL);
                let c = utf_ptr2char(rest);
                let in_class = match base_op {
                    IDENT | SIDENT => vim_is_idc(c),
                    KWORD | SKWORD => rex.iswordc(c),
                    _ => vim_isfilec(c),
                };
                if in_class && (testval || !ascii_isdigit(i32::from(b0))) {
                    scan += utfc_ptr2len(rest) as usize;
                } else if b0 == NUL {
                    if !repeat_crosses_line(rex, op) {
                        break;
                    }
                    rex.nextline();
                    scan = rex.input;
                    if gst.got_int.get() {
                        break;
                    }
                } else if rex.reg_line_lbr && b0 == b'\n' && with_nl(op) {
                    scan += 1;
                } else {
                    break;
                }
                count += 1;
            }
        }
        PRINT | SPRINT => {
            let testval = base_op == PRINT;
            while count < maxcount {
                let rest = rex.line_from(scan);
                let Some(&b0) = rest.first() else {
                    if !repeat_crosses_line(rex, op) {
                        break;
                    }
                    rex.nextline();
                    scan = rex.input;
                    if gst.got_int.get() {
                        break;
                    }
                    count += 1;
                    continue;
                };
                if vim_isprintc(utf_ptr2char(rest)) && (testval || !ascii_isdigit(i32::from(b0))) {
                    scan += utfc_ptr2len(rest) as usize;
                } else if rex.reg_line_lbr && b0 == b'\n' && with_nl(op) {
                    scan += 1;
                } else {
                    break;
                }
                count += 1;
            }
        }
        EXACTLY => {
            // A multi-byte character uses MULTIBYTECODE, so this is one byte.
            let line = rex.line();
            if rex.reg_ic {
                let cu = mb_toupper(i32::from(opnd[0]));
                let cl = mb_tolower(i32::from(opnd[0]));
                while count < maxcount
                    && line
                        .get(scan)
                        .is_some_and(|&b| i32::from(b) == cu || i32::from(b) == cl)
                {
                    count += 1;
                    scan += 1;
                }
            } else {
                while count < maxcount && line.get(scan) == Some(&opnd[0]) {
                    count += 1;
                    scan += 1;
                }
            }
        }
        MULTIBYTECODE => {
            let len = crate::mbyte::utfc_slice2len(opnd);
            if len > 1 {
                let cf = if rex.reg_ic {
                    utf_fold(crate::mbyte::utf_slice2char(opnd))
                } else {
                    0
                };
                let line = rex.line();
                while count < maxcount {
                    let rest = &line[scan.min(line.len())..];
                    if (utfc_ptr2len(rest) as usize) < len
                        || (rest[..len] != opnd[..len]
                            && (!rex.reg_ic || utf_fold(utf_ptr2char(rest)) != cf))
                    {
                        break;
                    }
                    scan += len;
                    count += 1;
                }
            }
        }
        ANYOF | ANYBUT => {
            let is_anyof = base_op == ANYOF;
            let opnd_bytes = crate::memory::bytes_until_nul(opnd);
            while count < maxcount {
                let rest = rex.line_from(scan);
                let Some(&b0) = rest.first() else {
                    if !repeat_crosses_line(rex, op) {
                        break;
                    }
                    rex.nextline();
                    scan = rex.input;
                    if gst.got_int.get() {
                        break;
                    }
                    count += 1;
                    continue;
                };
                if rex.reg_line_lbr && b0 == b'\n' && with_nl(op) {
                    scan += 1;
                } else {
                    let len = utfc_ptr2len(rest);
                    let c = if len > 1 {
                        utf_ptr2char(rest)
                    } else {
                        i32::from(b0)
                    };
                    if rex.cstrchr(opnd_bytes, c).is_none() == is_anyof {
                        break;
                    }
                    scan += len as usize;
                }
                count += 1;
            }
        }
        NEWL => {
            while count < maxcount {
                let b0 = rex.input_byte();
                if !((b0 == NUL
                    && rex.lnum <= rex.reg_maxline
                    && !rex.reg_line_lbr
                    && rex.reg_multi())
                    || (b0 == b'\n' && rex.reg_line_lbr))
                {
                    break;
                }
                count += 1;
                if rex.reg_line_lbr {
                    advance_reginput(rex);
                } else {
                    rex.nextline();
                }
                scan = rex.input;
                if gst.got_int.get() {
                    break;
                }
            }
        }
        _ => iemsg(tr(E_RE_CORR)),
    }

    rex.input = scan;
    count as i32
}

/// Which position of a group a `\(`, `\)`, `\z(` or `\z)` sets (`\zs` and `\ze` set those of
/// group 0).
#[derive(Clone, Copy, PartialEq, Eq)]
enum GroupEdge {
    Start,
    End,
    ZStart,
    ZEnd,
}

/// Puts back a group position that matching replaced.
#[derive(Clone, Copy)]
struct GroupUndo {
    edge: GroupEdge,
    no: u8,
    old: SaveSe,
}

impl GroupUndo {
    /// How to undo the change that the item of a `\(`, `\)`, `\z(` or `\z)` made to the `edge`
    /// of its group.
    #[inline(always)]
    fn of(edge: GroupEdge, rp: &Regitem) -> Self {
        Self {
            edge,
            no: rp.rs_no as u8,
            old: *rp.rs_un.sesave(),
        }
    }

    #[inline(always)]
    fn apply(&self, rex: &mut Regexec<'_>) {
        let no = usize::from(self.no);
        match self.edge {
            GroupEdge::Start => restore_se_m(rex, &self.old, no, false),
            GroupEdge::End => restore_se_m(rex, &self.old, no, true),
            GroupEdge::ZStart => restore_se_z(rex, &self.old, no, false),
            GroupEdge::ZEnd => restore_se_z(rex, &self.old, no, true),
        }
    }
}

/// Undoes the group changes on `trail` after its first `len`, newest first.
#[inline(always)]
fn undo_groups(rex: &mut Regexec<'_>, trail: &mut Vec<GroupUndo>, len: usize) {
    if trail.len() > len {
        undo_groups_after(rex, trail, len);
    }
}

/// Not inlined, like [`behind_step_back`]: only sub-patterns that set groups need it.
#[inline(never)]
fn undo_groups_after(rex: &mut Regexec<'_>, trail: &mut Vec<GroupUndo>, len: usize) {
    for undo in trail.drain(len..).rev() {
        undo.apply(rex);
    }
}

struct BtRegStack {
    items: Vec<Regitem>,
    stars: Vec<Regstar>,
    behinds: Vec<Regbehind>,
    behinds_len: usize,
    backpos: Vec<Backpos>,
    max_bytes: usize,
    /// How long `trail` was when each [`RS_NOMATCH`] item (`\@=`, `\@!` or `\@>`) was pushed.
    marks: Vec<usize>,
    /// How to undo each group change made while a `\@=`, `\@!`, `\@>` or look-behind is open,
    /// oldest first. An undo stays here after the item of its change is popped because the
    /// operand matched: the sub-pattern may still fail, or matching may back out past it.
    trail: Vec<GroupUndo>,
    /// How many `\@=`, `\@!`, `\@>` and look-behinds are open.
    open: usize,
}

impl BtRegStack {
    const fn new() -> Self {
        Self {
            items: Vec::new(),
            stars: Vec::new(),
            behinds: Vec::new(),
            behinds_len: 0,
            backpos: Vec::new(),
            max_bytes: (1000usize) << 10,
            marks: Vec::new(),
            trail: Vec::new(),
            open: 0,
        }
    }

    #[inline(always)]
    fn byte_len(&self) -> usize {
        self.items.len() * size_of::<Regitem>()
            + self.stars.len() * size_of::<Regstar>()
            + self.behinds_len * size_of::<Regbehind>()
            + self.marks.len() * size_of::<usize>()
            + self.trail.len() * size_of::<GroupUndo>()
    }

    /// The bytes allocated for the stack (upstream's `regstack.ga_maxlen`).
    fn allocated_bytes(&self) -> usize {
        self.items.capacity() * size_of::<Regitem>()
            + self.stars.capacity() * size_of::<Regstar>()
            + self.behinds.capacity() * size_of::<Regbehind>()
            + self.marks.capacity() * size_of::<usize>()
            + self.trail.capacity() * size_of::<GroupUndo>()
    }

    #[inline(always)]
    fn clear(&mut self) {
        self.items.clear();
        self.stars.clear();
        self.behinds_len = 0;
        self.marks.clear();
        self.trail.clear();
        self.open = 0;
        if !self.backpos.is_empty() {
            self.backpos.clear();
        }
    }

    #[inline(always)]
    fn push_behind(&mut self) {
        let idx = self.behinds_len;
        if idx == self.behinds.len() {
            self.behinds.push(Regbehind::default());
        }
        self.behinds_len = idx + 1;
        self.behinds[idx].trail_len = self.trail.len();
        self.open += 1;
    }

    #[inline(always)]
    fn pop_behind(&mut self) {
        self.behinds_len -= 1;
        self.open -= 1;
    }

    #[inline(always)]
    fn push(&mut self, state: Regstate, scan: Option<usize>) -> Option<&mut Regitem> {
        if self.byte_len() >= self.max_bytes {
            emsg(tr(E_PATTERN_USES_MORE_MEMORY_THAN_MAXMEMPATTERN));
            return None;
        }
        self.items.push(Regitem {
            rs_state: state,
            rs_no: 0,
            rs_scan: scan,
            rs_un: RegItemSave::new(),
        });
        self.items.last_mut()
    }

    #[inline(always)]
    fn pop(&mut self, scan: &mut Option<usize>) {
        let rp = self.items.pop().unwrap();
        *scan = rp.rs_scan;
    }

    /// Records how to undo the change that the group item on top made, while a sub-pattern is
    /// open.
    #[inline(always)]
    fn record_group(&mut self, edge: GroupEdge) {
        if self.open > 0 {
            self.push_undo(edge);
        }
    }

    /// Not inlined: only groups inside sub-patterns need it.
    #[inline(never)]
    fn push_undo(&mut self, edge: GroupEdge) {
        let undo = GroupUndo::of(edge, self.items.last().unwrap());
        self.trail.push(undo);
    }

    /// Pops the item of a `\(`, `\)`, `\z(` or `\z)` that set the `edge` of a group. When
    /// matching backs out, puts back the position it replaced and drops the undo that
    /// [`Self::record_group`] recorded, which is the last one on the trail.
    #[inline(always)]
    fn pop_group(
        &mut self,
        rex: &mut Regexec<'_>,
        status: RegAction,
        edge: GroupEdge,
        scan: &mut Option<usize>,
    ) {
        let rp = self.items.pop().unwrap();
        *scan = rp.rs_scan;
        if status == RegAction::NoMatch {
            GroupUndo::of(edge, &rp).apply(rex);
            if self.open > 0 {
                let undo = self.trail.pop();
                debug_assert!(undo.is_some_and(|u| u.edge == edge && i16::from(u.no) == rp.rs_no));
            }
        }
    }
}

std::thread_local! {
    /// The stack of the last match, for the next to reuse. Boxed: a match takes it out, and
    /// moving a pointer costs less than moving the stack.
    static BT_REGSTACK: std::cell::RefCell<Option<Box<BtRegStack>>> = const {
        std::cell::RefCell::new(None)
    };
}

pub(super) fn free_bt_regstack() {
    BT_REGSTACK.with(|c| *c.borrow_mut() = None);
}

/// Moves the start of a look-behind one character back, to try again to match it so that it ends
/// where what follows starts. When matching buffer lines it may go to the end of the previous
/// line once, and the limit then counts from there. Returns false when it cannot go back.
///
/// Not inlined: it would add register pressure to the loop in [`regmatch_inner`] that every
/// pattern runs.
#[inline(never)]
fn behind_step_back(
    rex: &mut Regexec<'_>,
    backpos: &mut Vec<Backpos>,
    rp: &mut Regitem,
    bp: &mut Regbehind,
    limit: i64,
) -> bool {
    if limit > 0 && i64::from(bp.chars_back) >= limit {
        return false;
    }
    if !rex.reg_multi() {
        let col = rp.rs_un.regsave().rs_u.ptr.unwrap_or(0);
        if col == 0 {
            return false;
        }
        rp.rs_un.regsave_mut().rs_u.ptr = Some(rex.prev_char_col(col as Colnr) as usize);
    } else {
        let pos = rp.rs_un.regsave().rs_u.pos();
        if pos.col == 0 {
            let lnum = pos.lnum - 1;
            if pos.lnum < rex.behind_pos.rs_u.pos().lnum || rex.reg_firstlnum + lnum < 1 {
                return false;
            }
            rp.rs_un.regsave_mut().rs_u.pos_mut().lnum = lnum;
            reg_restore(rex, backpos, rp.rs_un.regsave());
            rp.rs_un.regsave_mut().rs_u.pos_mut().col = rex.getline_len(rex.lnum);
            bp.chars_back = 0;
            return true;
        }
        let col = if pos.lnum == rex.lnum {
            rex.prev_char_col(pos.col)
        } else {
            let line = rex.getline(pos.lnum).unwrap_or_default();
            pos.col - 1 - crate::mbyte::utf_head_off_slice(line.bytes(), pos.col as usize - 1)
        };
        rp.rs_un.regsave_mut().rs_u.pos_mut().col = col;
    }
    bp.chars_back += 1;
    true
}

fn regmatch_inner(
    rex: &mut Regexec<'_>,
    prog: &[u8],
    scan_ref: &mut Option<usize>,
    tm: Option<Proftime>,
    timed_out: &mut Option<&mut i32>,
    regstack: &mut BtRegStack,
) -> bool {
    {
        let gst = &input_state();
        let reg_nobreak = rex.reg_nobreak;
        let mut scan = *scan_ref;
        let mut status: RegAction;
        let mut tm_count: i32 = 0;
        let mut break_tick: u32 = 0;

        regstack.clear();

        loop {
            break_tick = break_tick.wrapping_add(1);
            if (break_tick & 63) == 0 && !reg_nobreak {
                fast_breakcheck();
            }

            loop {
                let Some(scan_idx) = scan else {
                    status = RegAction::Fail;
                    break;
                };
                if gst.got_int.get() {
                    status = RegAction::Fail;
                    break;
                }
                if let Some(tm_val) = tm {
                    tm_count += 1;
                    if tm_count == 100 {
                        tm_count = 0;
                        if profile_passed_limit(tm_val) {
                            if let Some(p) = timed_out {
                                **p = 1;
                            }
                            status = RegAction::Fail;
                            break;
                        }
                    }
                }
                status = RegAction::Cont;

                let mut next = regnext(prog, scan);
                let mut op = op_fn(prog, scan);

                if !rex.reg_line_lbr
                    && with_nl(op)
                    && rex.reg_multi()
                    && rex.input_byte() == NUL
                    && rex.lnum <= rex.reg_maxline
                {
                    rex.nextline();
                } else if rex.reg_line_lbr && with_nl(op) && rex.input_byte() == b'\n' {
                    advance_reginput(rex);
                } else {
                    if with_nl(op) {
                        op -= ADD_NL;
                    }
                    let b0 = rex.input_byte();
                    let c = if b0 < 0x80 {
                        i32::from(b0)
                    } else {
                        utf_ptr2char(rex.rest())
                    };
                    match op {
                        BOL => {
                            if rex.input != 0 {
                                status = RegAction::NoMatch;
                            }
                        }
                        EOL => {
                            if c != NUL as i32 {
                                status = RegAction::NoMatch;
                            }
                        }
                        RE_BOF => {
                            if rex.lnum != 0
                                || rex.input != 0
                                || (rex.reg_multi() && rex.reg_firstlnum > 1)
                            {
                                status = RegAction::NoMatch;
                            }
                        }
                        RE_EOF => {
                            if rex.lnum != rex.reg_maxline || c != NUL as i32 {
                                status = RegAction::NoMatch;
                            }
                        }
                        CURSOR => {
                            if !rex.reg_win_cursor.is_some_and(|cur| {
                                rex.lnum + rex.reg_firstlnum == cur.lnum
                                    && rex.input_col() == cur.col
                            }) {
                                status = RegAction::NoMatch;
                            }
                        }
                        RE_MARK => {
                            let opnd = operand_idx(scan_idx);
                            let mark = i32::from(prog[opnd]);
                            let cmp = i32::from(prog[opnd + 1]);
                            let fm = mark_get(
                                &rex.reg_buf,
                                &crate::globals::curwin(),
                                None,
                                K_MARK_BUF_LOCAL,
                                mark,
                            );
                            if fm.as_ref().is_none_or(|f| f.mark.lnum <= 0) {
                                status = RegAction::NoMatch;
                            } else {
                                let pos = fm.unwrap().mark;
                                let pos_col = if pos.lnum == rex.lnum + rex.reg_firstlnum
                                    && pos.col == MAXCOL
                                {
                                    rex.getline_len(pos.lnum - rex.reg_firstlnum)
                                } else {
                                    pos.col
                                };
                                let cur_col = rex.input_col();
                                let nomatch = if pos.lnum == rex.lnum + rex.reg_firstlnum {
                                    if pos_col == cur_col {
                                        cmp == b'<' as i32 || cmp == b'>' as i32
                                    } else if pos_col < cur_col {
                                        cmp != b'>' as i32
                                    } else {
                                        cmp != b'<' as i32
                                    }
                                } else if pos.lnum < rex.lnum + rex.reg_firstlnum {
                                    cmp != b'>' as i32
                                } else {
                                    cmp != b'<' as i32
                                };
                                if nomatch {
                                    status = RegAction::NoMatch;
                                }
                            }
                        }
                        RE_VISUAL => {
                            if !rex.match_visual() {
                                status = RegAction::NoMatch;
                            }
                        }
                        RE_LNUM => {
                            if !rex.reg_multi()
                                || !re_num_cmp(
                                    (rex.lnum + rex.reg_firstlnum) as u32,
                                    prog,
                                    scan_idx,
                                )
                            {
                                status = RegAction::NoMatch;
                            }
                        }
                        RE_COL => {
                            if !re_num_cmp((rex.input + 1) as u32, prog, scan_idx) {
                                status = RegAction::NoMatch;
                            }
                        }
                        RE_VCOL => {
                            let wp = if rex.reg_win.is_null() {
                                crate::globals::curwin()
                            } else {
                                rex.reg_win.clone()
                            };
                            let mut lnum = if rex.reg_multi() {
                                rex.reg_firstlnum + rex.lnum
                            } else {
                                1
                            };
                            if rex.reg_multi()
                                && (lnum <= 0 || lnum > wp.buf().b_ml.ml_line_count.get())
                            {
                                lnum = 1;
                            }
                            let vcol = crate::plines::win_linetabsize_bytes(
                                &wp,
                                lnum,
                                rex.line(),
                                rex.input_col(),
                            );
                            if !re_num_cmp((vcol as u32).wrapping_add(1), prog, scan_idx) {
                                status = RegAction::NoMatch;
                            }
                        }
                        BOW => {
                            if c == NUL as i32 {
                                status = RegAction::NoMatch;
                            } else {
                                let this_class = mb_get_class_tab(rex.rest(), &rex.reg_chartab);
                                if this_class <= 1 || rex.prev_class() == this_class {
                                    status = RegAction::NoMatch;
                                }
                            }
                        }
                        EOW => {
                            if rex.input == 0 {
                                status = RegAction::NoMatch;
                            } else {
                                let this_class = mb_get_class_tab(rex.rest(), &rex.reg_chartab);
                                let prev_class = rex.prev_class();
                                if this_class == prev_class || prev_class == 0 || prev_class == 1 {
                                    status = RegAction::NoMatch;
                                }
                            }
                        }
                        ANY => {
                            if c == NUL as i32 {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        IDENT => {
                            if !vim_is_idc(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        SIDENT => {
                            if ascii_isdigit(b0 as i32) || !vim_is_idc(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        KWORD => {
                            if !crate::charset::vim_iswordc_tab(c, &rex.reg_chartab) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        SKWORD => {
                            if ascii_isdigit(b0 as i32)
                                || !crate::charset::vim_iswordc_tab(c, &rex.reg_chartab)
                            {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        FNAME => {
                            if !vim_isfilec(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        SFNAME => {
                            if ascii_isdigit(b0 as i32) || !vim_isfilec(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        PRINT => {
                            if !vim_isprintc(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        SPRINT => {
                            if ascii_isdigit(b0 as i32) || !vim_isprintc(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        WHITE => {
                            if !ascii_iswhite(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        NWHITE => {
                            if c == NUL as i32 || ascii_iswhite(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        DIGIT => {
                            if !ri_digit(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        NDIGIT => {
                            if c == NUL as i32 || ri_digit(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        HEX => {
                            if !ri_hex(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        NHEX => {
                            if c == NUL as i32 || ri_hex(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        OCTAL => {
                            if !ri_octal(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        NOCTAL => {
                            if c == NUL as i32 || ri_octal(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        WORD => {
                            if !ri_word(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        NWORD => {
                            if c == NUL as i32 || ri_word(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        HEAD => {
                            if !ri_head(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        NHEAD => {
                            if c == NUL as i32 || ri_head(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        ALPHA => {
                            if !ri_alpha(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        NALPHA => {
                            if c == NUL as i32 || ri_alpha(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        LOWER => {
                            if !ri_lower(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        NLOWER => {
                            if c == NUL as i32 || ri_lower(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        UPPER => {
                            if !ri_upper(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        NUPPER => {
                            if c == NUL as i32 || ri_upper(c) {
                                status = RegAction::NoMatch;
                            } else {
                                advance_reginput(rex);
                            }
                        }
                        EXACTLY => {
                            let opnd = &prog[operand_idx(scan_idx)..];
                            let opnd_bytes = crate::memory::bytes_until_nul(opnd);
                            if opnd[0] != b0 && !rex.reg_ic {
                                status = RegAction::NoMatch;
                            } else if !opnd_bytes.is_empty() {
                                let len = if opnd_bytes.len() == 1 && !rex.reg_ic {
                                    1
                                } else {
                                    let mut l = opnd_bytes.len() as i32;
                                    if rex.cstrncmp(opnd_bytes, rex.rest(), &mut l) != 0 {
                                        status = RegAction::NoMatch;
                                    }
                                    l
                                };
                                if status != RegAction::NoMatch
                                    && {
                                        let rest = rex.rest();
                                        let after = rest.get(len as usize..).unwrap_or(&[]);
                                        utf_composinglike(rest, after, None)
                                    }
                                    && !rex.reg_icombine
                                    && op_fn(prog, next) != RE_COMPOSING
                                {
                                    status = RegAction::NoMatch;
                                }
                                if status != RegAction::NoMatch {
                                    rex.advance_input(len as usize);
                                }
                            }
                        }
                        ANYOF | ANYBUT => {
                            let q_slice = &prog[operand_idx(scan_idx)..];
                            let q_bytes = crate::memory::bytes_until_nul(q_slice);
                            let matched = rex.cstrchr(q_bytes, c);
                            if c == NUL as i32 || (matched.is_none() == (op == ANYOF)) {
                                status = RegAction::NoMatch;
                            } else {
                                // The composing characters of the first item must follow the
                                // base character, whichever item matched (as upstream does).
                                let q_utf = crate::mbyte::utf_slice2len(q_slice);
                                let q_utfc = crate::mbyte::utfc_slice2len(q_slice);
                                if q_utfc == q_utf {
                                    advance_reginput(rex);
                                } else {
                                    let marks = &q_slice[q_utf..q_utfc];
                                    let rest = rex.rest();
                                    let base = utf_ptr2len(rest) as usize;
                                    if rest.get(base..).unwrap_or_default().starts_with(marks) {
                                        // The match includes the rest of the character.
                                        let len = utfc_ptr2len(rest) as usize;
                                        rex.advance_input(len.max(base + marks.len()));
                                    } else {
                                        status = RegAction::NoMatch;
                                    }
                                }
                            }
                        }
                        MULTIBYTECODE => {
                            let opnd = &prog[operand_idx(scan_idx)..];
                            let mut len = crate::mbyte::utfc_slice2len(opnd) as i32;
                            if len < 2 {
                                status = RegAction::NoMatch;
                            } else {
                                let opndc = crate::mbyte::utf_slice2char(opnd);
                                if utf_iscomposing_legacy(opndc) {
                                    status = RegAction::NoMatch;
                                    let rest = rex.rest();
                                    let mut i: usize = 0;
                                    while i < rest.len() {
                                        let inpc = utf_ptr2char(&rest[i..]);
                                        if !utf_iscomposing_legacy(inpc) {
                                            if i > 0 {
                                                break;
                                            }
                                        } else if opndc == inpc {
                                            // Include all following composing chars.
                                            len = (i as i32) + utfc_ptr2len(&rest[i..]);
                                            status = RegAction::Match;
                                            break;
                                        }
                                        i += utf_ptr2len(&rest[i..]) as usize;
                                    }
                                    if status != RegAction::NoMatch {
                                        rex.advance_input(len as usize);
                                    }
                                } else if rex.cstrncmp(
                                    crate::memory::bytes_until_nul(opnd),
                                    rex.rest(),
                                    &mut len,
                                ) != 0
                                {
                                    status = RegAction::NoMatch;
                                } else {
                                    rex.advance_input(len as usize);
                                }
                            }
                        }
                        RE_COMPOSING => {
                            while utf_iscomposing_legacy(utf_ptr2char(rex.rest())) {
                                let step = utf_ptr2len(rex.rest()) as usize;
                                rex.advance_input(step);
                            }
                        }
                        NOTHING => {}
                        BACK => {
                            let idx = if let Some(pos) = regstack
                                .backpos
                                .iter()
                                .position(|item| item.bp_scan == scan_idx)
                            {
                                if reg_save_equal(rex, &regstack.backpos[pos].bp_pos) {
                                    status = RegAction::NoMatch;
                                }
                                pos
                            } else {
                                let pos = regstack.backpos.len();
                                regstack.backpos.push(Backpos {
                                    bp_scan: scan_idx,
                                    bp_pos: Regsave::new(),
                                });
                                pos
                            };
                            if status != RegAction::NoMatch {
                                let mut pos_val = Regsave::new();
                                reg_save(rex, regstack.backpos.len(), &mut pos_val);
                                regstack.backpos[idx].bp_pos = pos_val;
                            }
                        }
                        x if (MOPEN..=MOPEN + 9).contains(&x) => {
                            let no = (op - MOPEN) as usize;
                            rex.cleanup_subexpr();
                            if let Some(rp) = regstack.push(RS_MOPEN, scan) {
                                rp.rs_no = no as i16;
                                save_se_m(rex, rp.rs_un.sesave_mut(), no, false);
                                regstack.record_group(GroupEdge::Start);
                            } else {
                                status = RegAction::Fail;
                            }
                        }
                        NOPEN | NCLOSE => {
                            if regstack.push(RS_NOPEN, scan).is_none() {
                                status = RegAction::Fail;
                            }
                        }
                        x if (ZOPEN + 1..=ZOPEN + 9).contains(&x) => {
                            let no = (op - ZOPEN) as usize;
                            rex.cleanup_zsubexpr();
                            if let Some(rp) = regstack.push(RS_ZOPEN, scan) {
                                rp.rs_no = no as i16;
                                save_se_z(rex, rp.rs_un.sesave_mut(), no, false);
                                regstack.record_group(GroupEdge::ZStart);
                            } else {
                                status = RegAction::Fail;
                            }
                        }
                        x if (MCLOSE..=MCLOSE + 9).contains(&x) => {
                            let no = (op - MCLOSE) as usize;
                            rex.cleanup_subexpr();
                            if let Some(rp) = regstack.push(RS_MCLOSE, scan) {
                                rp.rs_no = no as i16;
                                save_se_m(rex, rp.rs_un.sesave_mut(), no, true);
                                regstack.record_group(GroupEdge::End);
                            } else {
                                status = RegAction::Fail;
                            }
                        }
                        x if (ZCLOSE + 1..=ZCLOSE + 9).contains(&x) => {
                            let no = (op - ZCLOSE) as usize;
                            rex.cleanup_zsubexpr();
                            if let Some(rp) = regstack.push(RS_ZCLOSE, scan) {
                                rp.rs_no = no as i16;
                                save_se_z(rex, rp.rs_un.sesave_mut(), no, true);
                                regstack.record_group(GroupEdge::ZEnd);
                            } else {
                                status = RegAction::Fail;
                            }
                        }
                        x if (BACKREF + 1..=BACKREF + 9).contains(&x) => {
                            let no = (op - BACKREF) as usize;
                            let mut len: i32 = 0;
                            rex.cleanup_subexpr();
                            if !rex.reg_multi() {
                                if let (Some(start), Some(end)) =
                                    (rex.rm_startp[no], rex.rm_endp[no])
                                {
                                    len = end.saturating_sub(start) as i32;
                                    if rex.cstrncmp(rex.line_from(start), rex.rest(), &mut len) != 0
                                    {
                                        status = RegAction::NoMatch;
                                    }
                                }
                            } else {
                                let spos = rex.rmm_startpos[no];
                                let epos = rex.rmm_endpos[no];
                                if spos.lnum >= 0 && epos.lnum >= 0 {
                                    if spos.lnum == rex.lnum && epos.lnum == rex.lnum {
                                        len = epos.col - spos.col;
                                        let sub = rex.line_from(spos.col as usize);
                                        if rex.cstrncmp(sub, rex.rest(), &mut len) != 0 {
                                            status = RegAction::NoMatch;
                                        }
                                    } else {
                                        let r = rex.match_with_backref(
                                            spos.lnum, spos.col, epos.lnum, epos.col, &mut len,
                                        );
                                        if r != RegAction::Match {
                                            status = r;
                                        }
                                    }
                                }
                            }
                            rex.advance_input(len as usize);
                        }
                        x if (ZREF + 1..=ZREF + 9).contains(&x) => {
                            rex.cleanup_zsubexpr();
                            let no = (op - ZREF) as usize;
                            let ext_in = RE_STATE.with(|s| s.re_extmatch_in.get());
                            if let Some(ref em) = ext_in
                                && let Some(ref m_cstr) = em.matches[no]
                            {
                                let mut len = m_cstr.as_bytes().len() as i32;
                                if rex.cstrncmp(m_cstr.to_bytes(), rex.rest(), &mut len) != 0 {
                                    status = RegAction::NoMatch;
                                } else {
                                    rex.advance_input(len as usize);
                                }
                            }
                        }
                        BRANCH => {
                            if op_fn(prog, next) != BRANCH {
                                next = Some(operand_idx(scan_idx));
                            } else if regstack.push(RS_BRANCH, scan).is_some() {
                                status = RegAction::Break;
                            } else {
                                status = RegAction::Fail;
                            }
                        }
                        BRACE_LIMITS => {
                            if op_fn(prog, next) == BRACE_SIMPLE {
                                rex.bl_minval = operand_min(prog, scan_idx);
                                rex.bl_maxval = operand_max(prog, scan_idx);
                            } else if op_fn(prog, next) >= BRACE_COMPLEX
                                && op_fn(prog, next) < BRACE_COMPLEX + 10
                            {
                                let no = (op_fn(prog, next) - BRACE_COMPLEX) as usize;
                                rex.brace_min[no] = operand_min(prog, scan_idx);
                                rex.brace_max[no] = operand_max(prog, scan_idx);
                                rex.brace_count[no] = 0;
                            } else {
                                internal_error(c"BRACE_LIMITS");
                                status = RegAction::Fail;
                            }
                        }
                        x if (BRACE_COMPLEX..=BRACE_COMPLEX + 9).contains(&x) => {
                            let no = (op - BRACE_COMPLEX) as usize;
                            let (bmin, bmax, bcount) = {
                                let c = rex.brace_count[no] + 1;
                                rex.brace_count[no] = c;
                                (rex.brace_min[no], rex.brace_max[no], c)
                            };

                            let bp_len = regstack.backpos.len();
                            if i64::from(bcount) <= bmin.min(bmax) {
                                if let Some(rp) = regstack.push(RS_BRCPLX_MORE, scan) {
                                    rp.rs_no = no as i16;
                                    reg_save(rex, bp_len, rp.rs_un.regsave_mut());
                                    next = Some(operand_idx(scan_idx));
                                } else {
                                    status = RegAction::Fail;
                                }
                            } else if bmin <= bmax {
                                if i64::from(bcount) <= bmax {
                                    if let Some(rp) = regstack.push(RS_BRCPLX_LONG, scan) {
                                        rp.rs_no = no as i16;
                                        reg_save(rex, bp_len, rp.rs_un.regsave_mut());
                                        next = Some(operand_idx(scan_idx));
                                    } else {
                                        status = RegAction::Fail;
                                    }
                                }
                            } else if i64::from(bcount) <= bmin {
                                if let Some(rp) = regstack.push(RS_BRCPLX_SHORT, scan) {
                                    reg_save(rex, bp_len, rp.rs_un.regsave_mut());
                                } else {
                                    status = RegAction::Fail;
                                }
                            }
                        }
                        BRACE_SIMPLE | STAR | PLUS => {
                            let (minval, maxval) = if op != BRACE_SIMPLE {
                                (if op == STAR { 0 } else { 1 }, i64::from(MAX_LIMIT))
                            } else {
                                (rex.bl_minval, rex.bl_maxval)
                            };

                            let count =
                                i64::from(regrepeat(rex, gst, prog, operand_idx(scan_idx), maxval));
                            if gst.got_int.get() {
                                status = RegAction::Fail;
                                break;
                            }
                            if if minval <= maxval {
                                count >= minval
                            } else {
                                count >= maxval
                            } {
                                let (nextb, nextb_ic) = if let Some(next_idx) = next
                                    && op_fn(prog, next) == EXACTLY
                                {
                                    let nb = i32::from(prog[operand_idx(next_idx)]);
                                    let nb_ic = if rex.reg_ic {
                                        if mb_isupper(nb) {
                                            mb_tolower(nb)
                                        } else {
                                            mb_toupper(nb)
                                        }
                                    } else {
                                        nb
                                    };
                                    (nb, nb_ic)
                                } else {
                                    (NUL as i32, NUL as i32)
                                };
                                if regstack
                                    .push(
                                        if minval <= maxval {
                                            RS_STAR_LONG
                                        } else {
                                            RS_STAR_SHORT
                                        },
                                        scan,
                                    )
                                    .is_some()
                                {
                                    regstack.stars.push(Regstar {
                                        nextb,
                                        nextb_ic,
                                        count,
                                        minval,
                                        maxval,
                                    });
                                    status = RegAction::Break;
                                } else {
                                    status = RegAction::Fail;
                                }
                            } else {
                                status = RegAction::NoMatch;
                            }
                        }
                        NOMATCH | MATCH | SUBPAT => {
                            let bp_len = regstack.backpos.len();
                            if let Some(rp) = regstack.push(RS_NOMATCH, scan) {
                                rp.rs_no = op as i16;
                                reg_save(rex, bp_len, rp.rs_un.regsave_mut());
                                regstack.marks.push(regstack.trail.len());
                                regstack.open += 1;
                                next = Some(operand_idx(scan_idx));
                            } else {
                                status = RegAction::Fail;
                            }
                        }
                        BEHIND | NOBEHIND => {
                            let bp_len = regstack.backpos.len();
                            if let Some(rp) = regstack.push(RS_BEHIND1, scan) {
                                rp.rs_no = op as i16;
                                reg_save(rex, bp_len, rp.rs_un.regsave_mut());
                                regstack.push_behind();
                            } else {
                                status = RegAction::Fail;
                            }
                        }
                        BHPOS => {
                            if rex.reg_multi() {
                                let bpos = rex.behind_pos.rs_u.pos();
                                if bpos.col != rex.input_col() || bpos.lnum != rex.lnum {
                                    status = RegAction::NoMatch;
                                }
                            } else if rex.behind_pos.rs_u.ptr != Some(rex.input) {
                                status = RegAction::NoMatch;
                            }
                        }
                        NEWL => {
                            if (c != NUL as i32
                                || !rex.reg_multi()
                                || rex.lnum > rex.reg_maxline
                                || rex.reg_line_lbr)
                                && (c != b'\n' as i32 || !rex.reg_line_lbr)
                            {
                                status = RegAction::NoMatch;
                            } else if rex.reg_line_lbr {
                                advance_reginput(rex);
                            } else {
                                rex.nextline();
                            }
                        }
                        END => {
                            // As in the NFA engine, the match does not end inside a character,
                            // before a composing character, unless "\Z" ignores them or the
                            // match ends at the start of a line.
                            status = if is_pattern_end(prog, scan_idx)
                                && !rex.reg_icombine
                                && rex.input != 0
                                && rex.input_byte() >= 0x80
                                && utf_iscomposing_legacy(utf_ptr2char(rex.rest()))
                            {
                                RegAction::NoMatch
                            } else {
                                RegAction::Match
                            };
                        }
                        _ => {
                            iemsg(tr(E_RE_CORR));
                            status = RegAction::Fail;
                        }
                    }
                }

                if status != RegAction::Cont {
                    break;
                }
                scan = next;
            }

            while !regstack.items.is_empty() && status != RegAction::Fail {
                let len_before = regstack.items.len();
                let rp_state = regstack.items.last().unwrap().rs_state;
                match rp_state {
                    RS_NOPEN => {
                        regstack.pop(&mut scan);
                    }
                    RS_MOPEN => regstack.pop_group(rex, status, GroupEdge::Start, &mut scan),
                    RS_ZOPEN => regstack.pop_group(rex, status, GroupEdge::ZStart, &mut scan),
                    RS_MCLOSE => regstack.pop_group(rex, status, GroupEdge::End, &mut scan),
                    RS_ZCLOSE => regstack.pop_group(rex, status, GroupEdge::ZEnd, &mut scan),
                    RS_BRANCH => {
                        if status == RegAction::Match {
                            regstack.pop(&mut scan);
                        } else {
                            if status != RegAction::Break {
                                let rp = regstack.items.last_mut().unwrap();
                                reg_restore(rex, &mut regstack.backpos, rp.rs_un.regsave());
                                scan = rp.rs_scan;
                            }
                            if scan.is_none() || op_fn(prog, scan) != BRANCH {
                                status = RegAction::NoMatch;
                                regstack.pop(&mut scan);
                            } else {
                                let bp_len = regstack.backpos.len();
                                let rp = regstack.items.last_mut().unwrap();
                                rp.rs_scan = regnext(prog, scan);
                                reg_save(rex, bp_len, rp.rs_un.regsave_mut());
                                scan = scan.map(operand_idx);
                            }
                        }
                    }
                    RS_BRCPLX_MORE => {
                        if status == RegAction::NoMatch {
                            let rp = regstack.items.last_mut().unwrap();
                            let rs_no = rp.rs_no as usize;
                            reg_restore(rex, &mut regstack.backpos, rp.rs_un.regsave());
                            rex.brace_count[rs_no] -= 1;
                        }
                        regstack.pop(&mut scan);
                    }
                    RS_BRCPLX_LONG => {
                        if status == RegAction::NoMatch {
                            let rp = regstack.items.last_mut().unwrap();
                            let rs_no = rp.rs_no as usize;
                            reg_restore(rex, &mut regstack.backpos, rp.rs_un.regsave());
                            rex.brace_count[rs_no] -= 1;
                            status = RegAction::Cont;
                        }
                        regstack.pop(&mut scan);
                        if status == RegAction::Cont {
                            scan = regnext(prog, scan);
                        }
                    }
                    RS_BRCPLX_SHORT => {
                        if status == RegAction::NoMatch {
                            let rp = regstack.items.last_mut().unwrap();
                            reg_restore(rex, &mut regstack.backpos, rp.rs_un.regsave());
                        }
                        regstack.pop(&mut scan);
                        if status == RegAction::NoMatch {
                            scan = scan.map(operand_idx);
                            status = RegAction::Cont;
                        }
                    }
                    RS_NOMATCH => {
                        regstack.open -= 1;
                        let mark = *regstack.marks.last().unwrap();
                        let rp = regstack.items.last_mut().unwrap();
                        if status
                            == if rp.rs_no == NOMATCH as i16 {
                                RegAction::Match
                            } else {
                                RegAction::NoMatch
                            }
                        {
                            // The sub-pattern fails: undo the groups its operand set.
                            undo_groups(rex, &mut regstack.trail, mark);
                            status = RegAction::NoMatch;
                        } else {
                            status = RegAction::Cont;
                            if rp.rs_no != SUBPAT as i16 {
                                reg_restore(rex, &mut regstack.backpos, rp.rs_un.regsave());
                            }
                        }
                        if status == RegAction::Cont && regstack.trail.len() > mark {
                            // The groups the operand set stand until matching backs out past
                            // here: keep the item to undo them then.
                            rp.rs_state = RS_RESTORE;
                            scan = regnext(prog, rp.rs_scan);
                        } else {
                            regstack.marks.pop();
                            regstack.pop(&mut scan);
                            if status == RegAction::Cont {
                                scan = regnext(prog, scan);
                            }
                        }
                    }
                    RS_RESTORE => {
                        let mark = regstack.marks.pop().unwrap();
                        if status == RegAction::NoMatch {
                            undo_groups(rex, &mut regstack.trail, mark);
                        }
                        regstack.pop(&mut scan);
                    }
                    RS_BEHIND1 => {
                        if status == RegAction::NoMatch {
                            regstack.pop(&mut scan);
                            regstack.pop_behind();
                        } else {
                            let bp_len = regstack.backpos.len();
                            let behind_idx = regstack.behinds_len - 1;
                            let bp = &mut regstack.behinds[behind_idx];
                            let rp = regstack.items.last_mut().unwrap();
                            reg_save(rex, bp_len, &mut bp.save_after);
                            bp.save_behind = rex.behind_pos;
                            bp.chars_back = 0;

                            rex.behind_pos = *rp.rs_un.regsave();
                            rp.rs_state = RS_BEHIND2;
                            reg_restore(rex, &mut regstack.backpos, rp.rs_un.regsave());

                            scan = rp.rs_scan.map(|idx| operand_idx(idx) + 4);
                        }
                    }
                    RS_BEHIND2 => {
                        let behind_idx = regstack.behinds_len - 1;
                        let bp = &mut regstack.behinds[behind_idx];
                        let rp = regstack.items.last_mut().unwrap();
                        if status == RegAction::Match && reg_save_equal(rex, &rex.behind_pos) {
                            rex.behind_pos = bp.save_behind;
                            if rp.rs_no == BEHIND as i16 {
                                reg_restore(rex, &mut regstack.backpos, &bp.save_after);
                            } else {
                                status = RegAction::NoMatch;
                                undo_groups(rex, &mut regstack.trail, bp.trail_len);
                            }
                            regstack.pop(&mut scan);
                            regstack.pop_behind();
                        } else {
                            // No match or a match that doesn't end where we want it.
                            let limit = operand_min(prog, rp.rs_scan.unwrap());
                            if behind_step_back(rex, &mut regstack.backpos, rp, bp, limit) {
                                reg_restore(rex, &mut regstack.backpos, rp.rs_un.regsave());
                                scan = rp.rs_scan.map(|idx| operand_idx(idx) + 4);

                                if status == RegAction::Match {
                                    status = RegAction::NoMatch;
                                    undo_groups(rex, &mut regstack.trail, bp.trail_len);
                                }
                            } else {
                                rex.behind_pos = bp.save_behind;
                                if rp.rs_no == NOBEHIND as i16 {
                                    reg_restore(rex, &mut regstack.backpos, &bp.save_after);
                                    status = RegAction::Match;
                                } else {
                                    // What follows matched, so it may have set groups.
                                    status = RegAction::NoMatch;
                                    undo_groups(rex, &mut regstack.trail, bp.trail_len);
                                }
                                regstack.pop(&mut scan);
                                regstack.pop_behind();
                            }
                        }
                    }
                    RS_STAR_LONG | RS_STAR_SHORT => {
                        let rst = regstack.stars.last_mut().unwrap();
                        let rp = regstack.items.last_mut().unwrap();
                        if status == RegAction::Match {
                            regstack.pop(&mut scan);
                            regstack.stars.pop();
                        } else {
                            if status != RegAction::Break {
                                reg_restore(rex, &mut regstack.backpos, rp.rs_un.regsave());
                            }
                            loop {
                                if status != RegAction::Break {
                                    if rp.rs_state == RS_STAR_LONG {
                                        rst.count -= 1;
                                        if rst.count < rst.minval {
                                            break;
                                        }
                                        if rex.input == 0 {
                                            if rex.lnum == 0 {
                                                status = RegAction::NoMatch;
                                                break;
                                            }
                                            rex.lnum -= 1;
                                            match rex.getline(rex.lnum) {
                                                Some(line) => rex.line = line,
                                                // Just in case regrepeat() didn't count right.
                                                None => break,
                                            }
                                            rex.input = rex.getline_len(rex.lnum) as usize;
                                            rex.breakcheck();
                                        } else {
                                            rex.input = rex.prev_char_col(rex.input_col()) as usize;
                                        }
                                    } else {
                                        if rst.count == rst.minval
                                            || regrepeat(
                                                rex,
                                                gst,
                                                prog,
                                                operand_idx(rp.rs_scan.unwrap()),
                                                1,
                                            ) == 0
                                        {
                                            break;
                                        }
                                        rst.count += 1;
                                    }
                                    if gst.got_int.get() {
                                        break;
                                    }
                                } else {
                                    status = RegAction::NoMatch;
                                }

                                let ib = rex.input_byte() as i32;
                                if rst.nextb == NUL as i32 || ib == rst.nextb || ib == rst.nextb_ic
                                {
                                    reg_save(rex, regstack.backpos.len(), rp.rs_un.regsave_mut());
                                    scan = regnext(prog, rp.rs_scan);
                                    status = RegAction::Cont;
                                    break;
                                }
                            }
                            if status != RegAction::Cont {
                                regstack.pop(&mut scan);
                                regstack.stars.pop();
                                status = RegAction::NoMatch;
                            }
                        }
                    }
                    _ => {}
                }

                if status == RegAction::Cont || regstack.items.len() == len_before {
                    break;
                }
            }

            if status == RegAction::Cont {
                continue;
            }

            if regstack.items.is_empty() || status == RegAction::Fail {
                if scan.is_none() {
                    iemsg(tr(E_RE_CORR));
                }
                return status == RegAction::Match;
            }
        }
    }
}

/// Tries to match `prog` at column `col` of the current line; returns 0 for no match, else
/// one more than the number of the line the match ends in.
fn regtry(
    rex: &mut Regexec<'_>,
    prog: &BtRegprog,
    col: Colnr,
    tm: Option<Proftime>,
    mut timed_out: Option<&mut i32>,
    regstack: &mut BtRegStack,
) -> i32 {
    let start = (col as usize).min(rex.line().len());
    rex.input = start;
    rex.need_clear_subexpr = true;
    rex.need_clear_zsubexpr = prog.reghasz == REX_SET as u8;

    let mut scan = Some(1usize);
    if !regmatch_inner(rex, &prog.program, &mut scan, tm, &mut timed_out, regstack) {
        return 0;
    }

    rex.cleanup_subexpr();
    if rex.reg_multi() {
        if rex.rmm_startpos[0].lnum < 0 {
            rex.rmm_startpos[0] = Lpos { lnum: 0, col };
        }
        if rex.rmm_endpos[0].lnum < 0 {
            rex.rmm_endpos[0] = rex.cur_pos();
        } else {
            // Use the line number of "\ze".
            rex.lnum = rex.rmm_endpos[0].lnum;
        }
    } else {
        if rex.rm_startp[0].is_none() {
            rex.rm_startp[0] = Some(start);
        }
        if rex.rm_endp[0].is_none() {
            rex.rm_endp[0] = Some(rex.input);
        }
    }

    RE_STATE.with(|s| s.re_extmatch_out.set(None));

    if prog.reghasz == REX_SET as u8 {
        rex.cleanup_zsubexpr();
        let mut extmatch = RegExtmatch::default();
        for i in 0..NSUBEXP {
            if rex.reg_multi() {
                // Only accept single line matches.
                let sz = rex.reg_startzpos[i];
                let ez = rex.reg_endzpos[i];
                if sz.lnum >= 0
                    && ez.lnum == sz.lnum
                    && ez.col >= sz.col
                    && let Some(line) = rex.getline(sz.lnum)
                {
                    let line = line.bytes();
                    let start = (sz.col as usize).min(line.len());
                    let end = (ez.col as usize).min(line.len());
                    extmatch.matches[i] = cstring_from_bytes(&line[start..end]);
                }
            } else if let (Some(start), Some(end)) = (rex.reg_startzp[i], rex.reg_endzp[i]) {
                let text = rex.line_from(start);
                extmatch.matches[i] =
                    cstring_from_bytes(&text[..end.saturating_sub(start).min(text.len())]);
            }
        }
        RE_STATE.with(|s| s.re_extmatch_out.set(Some(std::rc::Rc::new(extmatch))));
    }
    1 + rex.lnum
}

/// Matches `prog` from column `startcol` of the line or string `rex` matches.
pub(super) fn bt_regexec_both(
    prog: &BtRegprog,
    rex: &mut Regexec<'_>,
    startcol: Colnr,
    tm: Option<Proftime>,
    mut timed_out: Option<&mut i32>,
) -> i32 {
    let mut col = startcol;
    let mut retval: i32 = 0;

    if rex.reg_multi() {
        let Some(line) = rex.getline(0) else {
            iemsg(tr(E_NULL));
            return finish_bt_regexec(rex, retval, col, None);
        };
        rex.line = line;
    }

    if prog.program.first().copied() != Some(REGMAGIC as u8) {
        emsg(tr(E_RE_CORR));
        return finish_bt_regexec(rex, retval, col, None);
    }

    // If the start column is past the maximum column: no need to try.
    if rex.reg_maxcol > 0 && col >= rex.reg_maxcol {
        return finish_bt_regexec(rex, retval, col, None);
    }

    // If the pattern contains "\c" or "\C": overrule the value of reg_ic.
    if (prog.regflags & RF_ICASE) != 0 {
        rex.reg_ic = true;
    } else if (prog.regflags & RF_NOICASE) != 0 {
        rex.reg_ic = false;
    }

    // If the pattern contains "\Z": overrule the value of reg_icombine.
    if (prog.regflags & RF_ICOMBINE) != 0 {
        rex.reg_icombine = true;
    }

    // If there is a "must appear" string, look for it.
    if let Some(off) = prog.regmust_off {
        let regmust = crate::memory::bytes_until_nul(&prog.program[off..]);
        let c = crate::mbyte::utf_slice2char(regmust);
        let line = rex.line();
        let mut s = (col as usize).min(line.len());
        let found = loop {
            let rest = &line[s..];
            let found_off = if rex.reg_ic {
                rex.cstrchr(rest, c)
            } else {
                crate::strings::vim_strchr_pos(rest, c)
            };
            let Some(found_off) = found_off else {
                break false;
            };
            s += found_off;
            let mut len = prog.regmlen;
            if rex.cstrncmp(&line[s..], regmust, &mut len) == 0 {
                break true;
            }
            s += utfc_ptr2len(&line[s..]) as usize;
        };
        if !found {
            return finish_bt_regexec(rex, retval, col, None);
        }
    }

    rex.lnum = 0;
    let line = rex.line.clone();

    let mut regstack = BT_REGSTACK
        .with(|c| c.borrow_mut().take())
        .unwrap_or_else(|| Box::new(BtRegStack::new()));
    regstack.max_bytes = (opt_vars().p_mmp.get().max(1) as usize) << 10;

    if prog.reganch != 0 {
        // An anchored match needs to be tried only once.
        let c = utf_ptr2char(rex.line_from(col as usize));
        if prog.regstart == NUL as i32
            || prog.regstart == c
            || (rex.reg_ic
                && (utf_fold(prog.regstart) == utf_fold(c)
                    || (c < 255
                        && prog.regstart < 255
                        && mb_tolower(prog.regstart) == mb_tolower(c))))
        {
            retval = regtry(rex, prog, col, tm, timed_out.as_deref_mut(), &mut regstack);
        }
    } else {
        let gst = &input_state();
        let mut tm_count: i32 = 0;
        while !gst.got_int.get() {
            if prog.regstart != NUL as i32 {
                // Skip to the character the match must start with.
                let Some(found_off) = rex.cstrchr(rex.line_from(col as usize), prog.regstart)
                else {
                    retval = 0;
                    break;
                };
                col += found_off as Colnr;
            } else if prog.regstart_class != 0 {
                let mask = prog.regstart_class;
                let Some(found_off) = rex
                    .line_from(col as usize)
                    .iter()
                    .position(|&b| (RI_CLASS_TAB[b as usize] & mask) != 0)
                else {
                    retval = 0;
                    break;
                };
                col += found_off as Colnr;
            }

            // Check for the maximum column to try.
            if rex.reg_maxcol > 0 && col >= rex.reg_maxcol {
                retval = 0;
                break;
            }

            retval = regtry(rex, prog, col, tm, timed_out.as_deref_mut(), &mut regstack);
            if retval > 0 {
                break;
            }

            // A failed try may have moved to another line.
            if rex.lnum != 0 {
                rex.lnum = 0;
                rex.line = line.clone();
            }
            let rest = rex.line_from(col as usize);
            if rest.is_empty() {
                break;
            }
            col += utfc_ptr2len(rest);

            // Check for a timeout once in twenty times to avoid overhead.
            if let Some(tm_val) = tm {
                tm_count += 1;
                if tm_count == 20 {
                    tm_count = 0;
                    if profile_passed_limit(tm_val) {
                        if let Some(ref mut p) = timed_out {
                            **p = 1;
                        }
                        break;
                    }
                }
            }
        }
    }

    finish_bt_regexec(rex, retval, col, Some(regstack))
}

fn finish_bt_regexec(
    rex: &mut Regexec<'_>,
    retval: i32,
    col: Colnr,
    regstack: Option<Box<BtRegStack>>,
) -> i32 {
    if let Some(mut st) = regstack {
        if st.allocated_bytes() > REGSTACK_INITIAL as usize {
            st.items = Vec::new();
            st.stars = Vec::new();
            st.behinds = Vec::new();
            st.marks = Vec::new();
            st.trail = Vec::new();
        }
        if st.backpos.capacity() > BACKPOS_INITIAL as usize {
            st.backpos = Vec::new();
        }
        BT_REGSTACK.with(|c| *c.borrow_mut() = Some(st));
    }

    if retval > 0 {
        // Make sure the end is never before the start; "\zs" and "\ze" can do that. The start
        // may come from "\zs", so also return the column where the whole pattern matched.
        if rex.reg_multi() {
            let start = rex.rmm_startpos[0];
            let end = rex.rmm_endpos[0];
            if end.lnum < start.lnum || (end.lnum == start.lnum && end.col < start.col) {
                rex.rmm_endpos[0] = start;
            }
            rex.rmm_matchcol = col;
        } else {
            if let (Some(start), Some(end)) = (rex.rm_startp[0], rex.rm_endp[0])
                && end < start
            {
                rex.rm_endp[0] = Some(start);
            }
            rex.rm_matchcol = col;
        }
    }

    retval
}
fn re_num_cmp(val: u32, prog: &[u8], scan: usize) -> bool {
    let n = operand_min(prog, scan) as u32;
    let cmp = operand_cmp(prog, scan);
    if cmp == b'>' {
        val > n
    } else if cmp == b'<' {
        val < n
    } else {
        val == n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frees_a_stack_that_a_match_grew() {
        let _guard = crate::globals::TEST_GLOBAL_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _editor = crate::main_init::test_editor();

        let mut rm = Regmatch::new();
        rm.set_prog(vim_regcomp(br"\%#=1\(a\|b\)*$", RE_MAGIC + RE_STRING));
        assert!(vim_regexec(&mut rm, &[b'a'; 200], 0));
        rm.free_prog();

        let allocated = BT_REGSTACK.with(|c| c.borrow().as_ref().map(|st| st.allocated_bytes()));
        assert!(allocated.is_some_and(|bytes| bytes <= REGSTACK_INITIAL as usize));
    }
}
