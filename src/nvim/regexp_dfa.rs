#![forbid(unsafe_code)]

//! A lazy DFA for the NFA programs that only read characters and test line and word
//! boundaries. It finds the match the NFA engine finds, in ASCII text, reading each byte with
//! a table lookup. Nero-only: upstream has no DFA.
//!
//! The NFA engine keeps its threads in priority order: it starts a thread at each position
//! until a match is found, keeps one thread per state, and when a thread matches it drops the
//! threads of lower priority. A DFA state here is the ordered list of states the threads
//! reached (before following their empty transitions), the class of the character before the
//! position and whether new threads still start. Its transitions follow the empty transitions
//! in priority order knowing the character after the position, which decides `^`, `$`, `\<`
//! and `\>`. States and transitions are built when text needs them and kept with the program.
//!
//! One pass over the text finds where the match ends. The match starts at the earliest
//! position from which the program alone matches up to there. A byte from 0x80 makes the DFA
//! give the text to the NFA engine.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use super::*;

/// Most states a program may have to run on the DFA.
const MAX_NFA_STATES: usize = 2000;
/// Most DFA states kept for a program.
const MAX_DFA_STATES: usize = 4096;
/// How often a program may run out of DFA states before it stops using the DFA.
const MAX_OVERFLOWS: u32 = 3;
/// How much work a program does with the NFA engine before it gets a DFA, which costs more to
/// build than a few short matches: [`MATCH_WORK`] per match plus the bytes the match reads,
/// when known. Debug builds use the DFA at once and check each of its matches against the NFA
/// engine.
const ACTIVATE_AFTER: usize = 512;
const MATCH_WORK: usize = 64;
/// Most states one transition may visit; following empty transitions can take exponential
/// time, as it does in the NFA engine at each character.
const MAX_CLOSE_WORK: usize = 1 << 16;
/// The NFA engine of the automatic engine gives up on a match that takes `NFA_MAX_STATES`
/// steps, one per position it reads plus one at the end; the DFA leaves such matches to it.
const NFA_STEPS: usize = NFA_MAX_STATES as usize - 3;

/// A transition not built yet.
const UNKNOWN: u32 = u32::MAX;
/// Set in a transition when the pattern matches at the position the transition leaves.
const MATCH_BIT: u32 = 1 << 30;
const STATE_MASK: u32 = MATCH_BIT - 1;
/// The state from which nothing can match.
const DEAD: u32 = STATE_MASK;

/// The class of the character before a position at the start of the line; the other classes
/// are those of `mb_get_class_tab()`: 0 for blanks, 1 for punctuation, 2 for word characters.
const AT_START: u8 = 3;
/// State `IDLE + class` has no threads and starts a thread at each position.
const IDLE: u32 = 0;
/// State `ANCHORED + class` starts one thread, at its position only.
const ANCHORED: u32 = 4;

/// What the DFA found.
pub(crate) enum DfaRun {
    NoMatch,
    /// The whole match, as start and end byte offsets.
    Match(usize, usize),
    /// The DFA cannot tell: match with the NFA engine.
    Declined,
}

/// Why the DFA stopped without an answer.
enum Stop {
    /// It has too many states.
    Full,
    /// The text has a byte from 0x80, or is so long the NFA engine may give up.
    Text,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The program was not used often enough yet.
    Cold,
    /// The program uses something the DFA does not handle.
    Off,
    On,
}

/// The DFA of a program, built when the program did some work.
pub(crate) struct DfaCache {
    mode: Cell<Mode>,
    work: Cell<usize>,
    dfa: RefCell<Option<Box<Dfa>>>,
}

impl DfaCache {
    pub(crate) const fn new() -> Self {
        Self {
            mode: Cell::new(Mode::Cold),
            work: Cell::new(0),
            dfa: RefCell::new(None),
        }
    }

    /// Forgets the DFA of a program that changed.
    pub(crate) fn clear(&self) {
        self.mode.set(Mode::Cold);
        self.work.set(0);
        *self.dfa.borrow_mut() = None;
    }

    /// Whether the DFA does not handle the program.
    #[inline]
    pub(crate) fn is_off(&self) -> bool {
        self.mode.get() == Mode::Off
    }

    /// Whether `prog` is matched with the DFA, for a match that reads up to `len` bytes, or 0
    /// when that is not known: when the DFA handles it and it did enough work.
    #[inline]
    pub(crate) fn enabled(&self, prog: &NfaRegprog, len: usize) -> bool {
        match self.mode.get() {
            Mode::On => true,
            Mode::Off => false,
            Mode::Cold => self.warm_up(prog, len),
        }
    }

    #[cold]
    fn warm_up(&self, prog: &NfaRegprog, len: usize) -> bool {
        let work = self.work.get().saturating_add(len + MATCH_WORK);
        self.work.set(work);
        if !cfg!(debug_assertions) && work < ACTIVATE_AFTER {
            return false;
        }
        let dfa = Dfa::new(prog);
        self.mode
            .set(if dfa.is_some() { Mode::On } else { Mode::Off });
        *self.dfa.borrow_mut() = dfa.map(Box::new);
        self.mode.get() == Mode::On
    }

    /// Matches `prog`, which is [`DfaCache::enabled`], in `text` from byte `col`, with `reg_ic`
    /// for 'ignorecase' and the 'iskeyword' table `chartab`.
    pub(crate) fn run(
        &self,
        prog: &NfaRegprog,
        text: &[u8],
        col: usize,
        reg_ic: bool,
        chartab: &[u64; 4],
    ) -> DfaRun {
        let mut slot = self.dfa.borrow_mut();
        let Some(dfa) = slot.as_deref_mut() else {
            return DfaRun::Declined;
        };
        // The NFA engine fails when its lists of threads need more memory than
        // 'maxmempattern' allows; the DFA only runs where they cannot.
        if dfa.mmp_bound_kb >= opt_vars().p_mmp.get() {
            return DfaRun::Declined;
        }
        let key = Key::new(reg_ic, chartab);
        if dfa.key != Some(key) {
            dfa.set_tables(&prog.state, key, chartab);
        }
        match dfa.search(&prog.state, text, col, reg_ic) {
            Ok(run) => run,
            Err(Stop::Text) => DfaRun::Declined,
            Err(Stop::Full) => {
                dfa.overflows += 1;
                if dfa.overflows >= MAX_OVERFLOWS {
                    *slot = None;
                    self.mode.set(Mode::Off);
                } else {
                    dfa.key = None;
                }
                DfaRun::Declined
            }
        }
    }
}

/// What the tables of a DFA depend on: 'ignorecase', and the ASCII half of the 'iskeyword',
/// 'isfname' and 'isident' tables.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    reg_ic: bool,
    tables: [u64; 6],
}

impl Key {
    fn new(reg_ic: bool, chartab: &[u64; 4]) -> Self {
        let (isf, isi) = crate::charset::get_global_chartabs();
        Self {
            reg_ic,
            tables: [chartab[0], chartab[1], isf[0], isf[1], isi[0], isi[1]],
        }
    }
}

/// An NFA state that reads a character.
#[derive(Clone, Copy)]
struct Reader {
    state: usize,
    /// The state a thread goes to after the character.
    next: Option<usize>,
}

/// A DFA state.
#[derive(Clone, PartialEq, Eq, Hash)]
struct DState {
    /// The NFA states of the threads, highest priority first.
    roots: Box<[u32]>,
    /// The class of the character before the position, or [`AT_START`].
    prev: u8,
    /// Whether a thread starts at the position, with the lowest priority.
    start: bool,
}

struct Dfa {
    /// The first state of the pattern.
    start: usize,
    /// A character every match starts with, or NUL.
    regstart: i32,
    /// Whether the NFA engine gives up on long matches: the automatic engine does.
    gives_up: bool,
    readers: Vec<Reader>,
    /// The index in `readers` of each NFA state that reads a character.
    reader_of: Vec<u32>,
    /// The memory in KiB the lists of threads of the NFA engine may need at most.
    mmp_bound_kb: i64,
    overflows: u32,

    /// The character tables the fields below were built for.
    key: Option<Key>,
    /// The class of each byte. The ASCII characters come first, then the end of the line, then
    /// the bytes from 0x80, which have no transitions.
    byte_class: [u8; 256],
    nclass: usize,
    /// For each class: the class of its characters for `\<` and `\>`.
    class_word: Vec<u8>,
    /// For each class: `words` words with a bit for each reader that accepts its characters.
    class_accept: Vec<u64>,
    words: usize,
    states: Vec<DState>,
    index: HashMap<DState, u32, crate::memory::FxBuildHasher>,
    /// The transitions of each state, `nclass` of them.
    trans: Vec<u32>,

    /// Space for building transitions.
    mark: Vec<u32>,
    stamp: u32,
    stack: Vec<usize>,
    roots: Vec<usize>,
    found: Vec<usize>,
    next: Vec<usize>,
}

impl Dfa {
    /// The DFA of `prog`, or `None` when `prog` uses something it does not handle.
    fn new(prog: &NfaRegprog) -> Option<Self> {
        if (prog.regflags & RF_ICOMBINE) != 0
            || prog.has_zend
            || prog.has_backref
            || prog.reghasz == REX_SET
            || prog.match_text.is_some()
        {
            return None;
        }
        let n = usize::try_from(prog.nstate).ok()?;
        if n > MAX_NFA_STATES {
            return None;
        }
        let states = prog.state.get(..n)?;
        let first = states.get(prog.start?)?;
        if first.c != NFA_MOPEN {
            return None;
        }
        let start = first.out?;

        let mut readers = Vec::new();
        let mut reader_of = vec![u32::MAX; n];
        let mut seen = vec![false; n];
        let mut todo = vec![start];
        while let Some(x) = todo.pop() {
            if *seen.get(x)? {
                continue;
            }
            seen[x] = true;
            let st = states[x];
            let next = match st.c {
                NFA_MATCH => None,
                NFA_SPLIT => {
                    todo.extend(st.out1);
                    st.out
                }
                NFA_EMPTY | NFA_NOPEN | NFA_NCLOSE | NFA_MCLOSE | NFA_BOL | NFA_EOL | NFA_BOW
                | NFA_EOW => st.out,
                c => {
                    let next = match c {
                        NFA_START_COLL | NFA_START_NEG_COLL => {
                            collection_end(states, &st)?;
                            states[st.out1?].out
                        }
                        NFA_ANY | NFA_IDENT..=NFA_NUPPER_IC => st.out,
                        c if c > 0 => st.out,
                        _ => return None,
                    };
                    reader_of[x] = readers.len() as u32;
                    readers.push(Reader { state: x, next });
                    next
                }
            };
            todo.extend(next);
        }
        if has_empty_loop(states, &seen) {
            return None;
        }

        // The lists of the NFA engine hold at most two threads per state, one of them added by
        // a "^", "$", "\<" or "\>" before its other copy runs; they grow by half plus 50.
        let most_threads = 2 * n + 2;
        let bytes = (3 * most_threads + 50) * std::mem::size_of::<NfaThread>();
        Some(Self {
            start,
            regstart: prog.regstart,
            gives_up: prog.re_engine == AUTOMATIC_ENGINE as u32,
            readers,
            reader_of,
            mmp_bound_kb: i64::try_from(bytes >> 10).unwrap_or(i64::MAX),
            overflows: 0,
            key: None,
            byte_class: [0; 256],
            nclass: 0,
            class_word: Vec::new(),
            class_accept: Vec::new(),
            words: 0,
            states: Vec::new(),
            index: HashMap::default(),
            trans: Vec::new(),
            mark: vec![0; n],
            stamp: 0,
            stack: Vec::new(),
            roots: Vec::new(),
            found: Vec::new(),
            next: Vec::new(),
        })
    }

    /// Builds the byte classes for the character tables in `key` and forgets the states.
    fn set_tables(&mut self, states: &[NfaState], key: Key, chartab: &[u64; 4]) {
        let mut class_masks: Vec<(i32, u128)> = Vec::new();
        let masks: Vec<u128> = self
            .readers
            .iter()
            .map(|r| {
                let c = states[r.state].c;
                if !(NFA_IDENT..=NFA_NUPPER_IC).contains(&c) {
                    return accepted(states, r.state, key.reg_ic, chartab);
                }
                if let Some(&(_, m)) = class_masks.iter().find(|&&(k, _)| k == c) {
                    return m;
                }
                let m = accepted(states, r.state, key.reg_ic, chartab);
                class_masks.push((c, m));
                m
            })
            .collect();
        let word: [u8; 128] = std::array::from_fn(|b| mb_get_class_tab(&[b as u8], chartab) as u8);

        // ASCII characters are in one class when `\<`, `\>` and every reader treat them alike.
        let mut class = [0u8; 128];
        let mut nascii = refine(&mut class, 3, |b| usize::from(word[b]));
        for &m in &masks {
            nascii = refine(&mut class, 2, |b| (m >> b) as usize & 1);
        }
        let words = self.readers.len().div_ceil(64).max(1);
        let mut first = [u8::MAX; 128];
        for b in (0..128).rev() {
            first[usize::from(class[b])] = b as u8;
        }
        self.class_word.clear();
        self.class_accept.clear();
        for &b in &first[..nascii] {
            self.class_word.push(word[usize::from(b)]);
            for w in 0..words {
                let mut bits = 0u64;
                for (i, &m) in masks.iter().enumerate().skip(w * 64).take(64) {
                    bits |= (((m >> b) & 1) as u64) << (i % 64);
                }
                self.class_accept.push(bits);
            }
        }
        // The end of the line, class 0 with no character, and the bytes from 0x80.
        for _ in 0..2 {
            self.class_word.push(0);
            self.class_accept.extend(std::iter::repeat_n(0, words));
        }
        self.byte_class[..128].copy_from_slice(&class);
        self.byte_class[128..].fill(nascii as u8 + 1);
        self.nclass = nascii + 2;
        self.words = words;
        self.key = Some(key);

        self.states.clear();
        self.index.clear();
        self.trans.clear();
        let first = self.start;
        for (roots, starts) in [(&[][..], true), (&[first][..], false)] {
            for prev in 0..=AT_START {
                let id = self.intern(roots, prev, starts);
                debug_assert!(id.is_ok());
            }
        }
    }

    /// The class of the end of the line.
    #[inline(always)]
    fn end_class(&self) -> usize {
        self.nclass - 2
    }

    /// The byte class at position `p` of `text`.
    #[inline(always)]
    fn class_at(&self, text: &[u8], p: usize) -> usize {
        match text.get(p) {
            Some(&b) => usize::from(self.byte_class[usize::from(b)]),
            None => self.end_class(),
        }
    }

    /// The class of the character before position `p` of `text`.
    #[inline(always)]
    fn prev_at(&self, text: &[u8], p: usize) -> Result<u8, Stop> {
        let Some(p) = p.checked_sub(1) else {
            return Ok(AT_START);
        };
        let b = text[p];
        if b >= 0x80 {
            return Err(Stop::Text);
        }
        Ok(self.class_word[usize::from(self.byte_class[usize::from(b)])])
    }

    #[inline(always)]
    fn step(&mut self, states: &[NfaState], s: u32, class: usize) -> Result<u32, Stop> {
        let t = self.trans[s as usize * self.nclass + class];
        if t != UNKNOWN {
            Ok(t)
        } else {
            self.build(states, s, class)
        }
    }

    /// Finds the match in `text` from byte `col`.
    fn search(
        &mut self,
        states: &[NfaState],
        text: &[u8],
        col: usize,
        reg_ic: bool,
    ) -> Result<DfaRun, Stop> {
        let len = text.len();
        let mut p = col;
        let mut s = IDLE + u32::from(self.prev_at(text, p)?);
        // The last position without threads: the match cannot start before it.
        let mut idle_at = p;
        let mut first = None;
        let mut end = 0;
        let mut steps = 0;
        loop {
            let t = self.step(states, s, self.class_at(text, p))?;
            if t & MATCH_BIT != 0 {
                first.get_or_insert(p);
                end = p;
            }
            s = t & STATE_MASK;
            if s == DEAD || p == len {
                break;
            }
            steps += 1;
            if steps >= NFA_STEPS && self.gives_up {
                return Err(Stop::Text);
            }
            p += 1;
            if s < ANCHORED {
                // No threads: like the NFA engine, skip to a character the match can start
                // with.
                if self.regstart != NUL as i32 {
                    let Some(off) = cstrchr_ic(&text[p..], self.regstart, reg_ic) else {
                        break;
                    };
                    if off > 0 {
                        p += off;
                        s = IDLE + u32::from(self.prev_at(text, p)?);
                    }
                }
                idle_at = p;
            }
        }
        let Some(first) = first else {
            return Ok(DfaRun::NoMatch);
        };

        // The thread that matches started before the first match, at or after `idle_at`, and
        // at the earliest such position from which the pattern matches by `end`.
        let mut budget = 4 * (end - idle_at + 1) + 64;
        for from in idle_at..=first {
            let mut s = ANCHORED + u32::from(self.prev_at(text, from)?);
            let mut q = from;
            loop {
                if budget == 0 {
                    return Ok(DfaRun::Declined);
                }
                budget -= 1;
                let t = self.step(states, s, self.class_at(text, q))?;
                if t & MATCH_BIT != 0 {
                    return Ok(DfaRun::Match(from, end));
                }
                s = t & STATE_MASK;
                if s == DEAD || q >= end {
                    break;
                }
                q += 1;
            }
        }
        debug_assert!(
            false,
            "the DFA found where a match ends but not where it starts"
        );
        Ok(DfaRun::Declined)
    }

    /// Builds the transition of state `s` over a character of class `class`.
    #[cold]
    #[inline(never)]
    fn build(&mut self, states: &[NfaState], s: u32, class: usize) -> Result<u32, Stop> {
        if class == self.nclass - 1 {
            return Err(Stop::Text);
        }
        let state = &self.states[s as usize];
        let (prev, starts) = (state.prev, state.start);
        self.roots.clear();
        self.roots.extend(state.roots.iter().map(|&r| r as usize));
        if starts {
            self.roots.push(self.start);
        }
        let matched = self.close(states, prev, class)?;

        let mut t = if matched { MATCH_BIT } else { 0 };
        let still_starts = starts && !matched;
        if class == self.end_class() {
            t |= DEAD;
        } else {
            self.next_stamp();
            self.next.clear();
            for &x in &self.found {
                let r = self.reader_of[x] as usize;
                if self.class_accept[class * self.words + r / 64] & (1 << (r % 64)) != 0
                    && let Some(n) = self.readers[r].next
                    && self.mark[n] != self.stamp
                {
                    self.mark[n] = self.stamp;
                    self.next.push(n);
                }
            }
            if self.next.is_empty() && !still_starts {
                t |= DEAD;
            } else {
                let next = std::mem::take(&mut self.next);
                let id = self.intern(&next, self.class_word[class], still_starts);
                self.next = next;
                t |= id?;
            }
        }
        self.trans[s as usize * self.nclass + class] = t;
        Ok(t)
    }

    /// Follows the empty transitions from `self.roots` in priority order, at a position after
    /// a character of class `prev` and before one of byte class `class`, as the NFA engine adds
    /// states to its list and runs them. Puts the states that read a character in
    /// `self.found`. Returns whether a thread reaches the end of the pattern, which drops the
    /// threads of lower priority.
    fn close(&mut self, states: &[NfaState], prev: u8, class: usize) -> Result<bool, Stop> {
        self.next_stamp();
        self.found.clear();
        let at_end = class == self.end_class();
        let word = self.class_word[class];
        let mut work = 0;
        for i in 0..self.roots.len() {
            self.stack.push(self.roots[i]);
            while let Some(x) = self.stack.pop() {
                work += 1;
                if work > MAX_CLOSE_WORK {
                    self.stack.clear();
                    return Err(Stop::Full);
                }
                let st = states[x];
                // Like the NFA engine, follow the states that add no thread each time they are
                // reached, which can be from inside their own loop, and the others once.
                match st.c {
                    NFA_SPLIT => {
                        self.stack.extend(st.out1);
                        self.stack.extend(st.out);
                        continue;
                    }
                    NFA_EMPTY | NFA_NCLOSE | NFA_MCLOSE => {
                        self.stack.extend(st.out);
                        continue;
                    }
                    _ => {}
                }
                if self.mark[x] == self.stamp {
                    continue;
                }
                self.mark[x] = self.stamp;
                let out = match st.c {
                    NFA_MATCH => {
                        self.stack.clear();
                        return Ok(true);
                    }
                    NFA_NOPEN => st.out,
                    NFA_BOL => st.out.filter(|_| prev == AT_START),
                    NFA_EOL => st.out.filter(|_| at_end),
                    NFA_BOW => st.out.filter(|_| word == 2 && prev != 2),
                    NFA_EOW => st.out.filter(|_| prev == 2 && word != 2),
                    _ => {
                        self.found.push(x);
                        None
                    }
                };
                self.stack.extend(out);
            }
        }
        Ok(false)
    }

    fn next_stamp(&mut self) {
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.mark.fill(0);
            self.stamp = 1;
        }
    }

    /// The DFA state for `roots`, `prev` and `start`, added when new.
    fn intern(&mut self, roots: &[usize], prev: u8, start: bool) -> Result<u32, Stop> {
        let state = DState {
            roots: roots.iter().map(|&r| r as u32).collect(),
            prev,
            start,
        };
        if let Some(&id) = self.index.get(&state) {
            return Ok(id);
        }
        if self.states.len() >= MAX_DFA_STATES {
            return Err(Stop::Full);
        }
        let id = self.states.len() as u32;
        self.states.push(state.clone());
        self.index.insert(state, id);
        self.trans.extend(std::iter::repeat_n(UNKNOWN, self.nclass));
        Ok(id)
    }
}

/// Splits the byte classes in `class` by `label`, which is below `labels`, at most 3; returns
/// the new number of classes.
fn refine(class: &mut [u8; 128], labels: usize, label: impl Fn(usize) -> usize) -> usize {
    let mut ids = [u8::MAX; 128 * 3];
    let mut n = 0;
    for (b, c) in class.iter_mut().enumerate() {
        let k = usize::from(*c) * labels + label(b);
        if ids[k] == u8::MAX {
            ids[k] = n as u8;
            n += 1;
        }
        *c = ids[k];
    }
    n
}

/// The ASCII characters NFA state `x`, which reads a character, accepts, as bits.
fn accepted(states: &[NfaState], x: usize, reg_ic: bool, chartab: &[u64; 4]) -> u128 {
    let c = states[x].c;
    if c > 0 {
        // `char_matches()` for each ASCII character, without folding `c` each time.
        let mut m = if c < 128 { 1u128 << c } else { 0 };
        if reg_ic {
            let folded = utf_fold(c);
            for (b, &f) in ascii_folds().iter().enumerate().skip(1) {
                if f == folded {
                    m |= 1 << b;
                }
            }
        }
        return m;
    }
    (1..128)
        .filter(|&b| reads(states, x, b, reg_ic, chartab))
        .fold(0, |m, b| m | (1 << b))
}

/// `utf_fold()` of each ASCII character.
fn ascii_folds() -> &'static [i32; 128] {
    static FOLDS: std::sync::OnceLock<[i32; 128]> = std::sync::OnceLock::new();
    FOLDS.get_or_init(|| std::array::from_fn(|b| utf_fold(b as i32)))
}

/// Checks the items of the collection `coll`, `[]` or `[^]`: `Some` when the DFA handles them.
fn collection_end(states: &[NfaState], coll: &NfaState) -> Option<()> {
    if states.get(coll.out1?)?.c != NFA_END_COLL {
        return None;
    }
    let mut item = coll.out;
    for _ in 0..states.len() {
        let st = states.get(item?)?;
        item = match st.c {
            NFA_END_COLL => return Some(()),
            NFA_RANGE_MIN => {
                let max = states.get(st.out?)?;
                if max.c != NFA_RANGE_MAX {
                    return None;
                }
                max.out
            }
            NFA_CLASS_ALNUM..=NFA_CLASS_FNAME => st.out,
            c if c > 0 => st.out,
            _ => return None,
        };
    }
    None
}

/// Whether the states in `seen` loop through empty transitions that the NFA engine follows
/// without adding a thread, which it would follow until it gives up: the ones that
/// `Dfa::close` follows each time.
fn has_empty_loop(states: &[NfaState], seen: &[bool]) -> bool {
    let empty = |c: i32| matches!(c, NFA_SPLIT | NFA_EMPTY | NFA_NCLOSE | NFA_MCLOSE);
    // 1: on the path being followed, 2: done.
    let mut color = vec![0u8; states.len()];
    let mut path: Vec<(usize, u8)> = Vec::new();
    for root in 0..states.len() {
        if !seen[root] || color[root] != 0 || !empty(states[root].c) {
            continue;
        }
        color[root] = 1;
        path.push((root, 0));
        while let Some((x, edge)) = path.last_mut() {
            let st = states[*x];
            let target = match *edge {
                0 => st.out,
                1 if st.c == NFA_SPLIT => st.out1,
                _ => {
                    color[*x] = 2;
                    path.pop();
                    continue;
                }
            };
            *edge += 1;
            if let Some(t) = target
                && empty(states[t].c)
            {
                match color[t] {
                    0 => {
                        color[t] = 1;
                        path.push((t, 0));
                    }
                    1 => return true,
                    _ => {}
                }
            }
        }
    }
    false
}

/// Whether NFA state `x`, which reads a character, accepts character `c`, as the NFA engine
/// tests it.
fn reads(states: &[NfaState], x: usize, c: i32, reg_ic: bool, chartab: &[u64; 4]) -> bool {
    let st = &states[x];
    match st.c {
        NFA_ANY => c > 0,
        NFA_START_COLL | NFA_START_NEG_COLL => {
            c != NUL as i32 && collection_matches(states, st, c, reg_ic, chartab) == Some(true)
        }
        class @ NFA_IDENT..=NFA_NUPPER_IC => class_test(class, c, reg_ic, chartab),
        sc => char_matches(sc, c, reg_ic),
    }
}
