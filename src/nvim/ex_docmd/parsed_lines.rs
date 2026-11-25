//! Runs Ex command lines that run again and again from statements parsed once:
//! the lines of a `:while` or `:for` loop as it repeats, instead of passing
//! each one to `do_one_cmd()` and its expression to `eval0()`.
//!
//! A run takes the steps that `do_cmdline()` and `do_one_cmd()` take for a
//! line when nothing unusual is going on. It stops after a line that ends with
//! an error, an interrupt or an exception, and before a line it does not run,
//! so that they take the remaining steps as before. Setting
//! `NERO_NO_PARSED_LINES` in the environment turns runs off, so that tests can
//! compare both ways.
//!
//! The statements mirror `ex_let()`, `ex_if()`, `ex_else()`, `ex_endif()`,
//! `ex_while()`, `ex_endwhile()`, `ex_continue()` and `ex_break()`; MAINTAIN.md
//! lists what a change to those, to `do_cmdline()` or to `do_one_cmd()` needs
//! here.
#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CStr;
use std::rc::Rc;

use super::*;
use crate::charset::skipwhite_slice;
use crate::debugger::{dbg_clear_skipped, dbg_quiet};
use crate::eval::expr_tree::{Expr, is_simple_var};
use crate::eval::next_for_item;
use crate::eval::typval::{tv_clear, tv_get_number_chk};
use crate::eval::vars::{ex_let_vars, skip_var_list};
use crate::ex_eval::{
    CSF_ACTIVE, CSF_ELSE, CSF_FOR, CSF_TRUE, CSF_TRY, CSF_WHILE, CSL_HAD_CONT, CSL_HAD_ENDLOOP,
    CSL_HAD_LOOP, CSTACK_LEN, Cstack, check_skip, cleanup_conditionals, do_errthrow,
    rewind_conditionals,
};

/// A line parsed once.
pub(super) struct Line {
    stmt: Stmt,
    cmdidx: Cmdidx,
    /// The command after this one on the line, which `do_one_cmd()` leaves in
    /// the command line when this one runs without an error.
    next: NextCmd,
}

/// The command after a command on its line, if any.
type NextCmd = Option<Box<[u8]>>;

/// A line parsed: `None` when a run does not run it.
pub(super) type Parsed = Option<Rc<Line>>;

enum Stmt {
    /// `:let {var} {op} {expr}`: `arg` from the variable on, `text` from the
    /// expression on.
    Let {
        arg: Box<[u8]>,
        op: &'static CStr,
        text: Box<[u8]>,
        expr: Expr,
    },
    If {
        text: Box<[u8]>,
        expr: Expr,
    },
    Elseif {
        text: Box<[u8]>,
        expr: Expr,
    },
    Else,
    Endif,
    /// `:while` as the loop repeats.
    While {
        text: Box<[u8]>,
        expr: Expr,
    },
    /// `:for` as the loop repeats: `arg` from the variable on.
    For {
        arg: Box<[u8]>,
    },
    /// `:endwhile` (`CSF_WHILE`) or `:endfor` (`CSF_FOR`).
    EndLoop {
        csf: i32,
    },
    Continue,
    Break,
}

/// Command flags that allow a command in the sandbox and when text or the
/// buffer is locked.
const RUN_OK: u32 = EX_SBOXOK | EX_BUFLOCK_OK | EX_LOCK_OK;

impl Stmt {
    /// The flags its command has to have for a run to run it. With these and
    /// no range or "!", every check `do_one_cmd()` makes by the flags passes,
    /// so a run skips them.
    fn flags(&self) -> u32 {
        match self {
            Self::Let { .. }
            | Self::If { .. }
            | Self::Elseif { .. }
            | Self::While { .. }
            | Self::For { .. } => EX_EXTRA | EX_NOTRLCOM | RUN_OK,
            Self::Else | Self::Endif | Self::EndLoop { .. } | Self::Continue | Self::Break => {
                EX_TRLBAR | RUN_OK
            }
        }
    }
}

/// What a run did.
pub(super) enum Ran {
    /// Ran the lines up to the current one, after which `do_cmdline()` takes
    /// its steps; `has_next` is what `do_one_cmd()` returns for it.
    After { has_next: bool },
    /// Ran the lines before the current one, and the steps after them but for
    /// setting what to run next from `has_next` of the last one.
    Before { has_next: bool },
}

/// The state of `do_cmdline()` a loop run uses.
pub(super) struct Run<'a> {
    pub(super) cstack: &'a mut Box<Cstack>,
    pub(super) cookie: &'a RefCell<LoopCookie>,
    pub(super) current_line: &'a mut i32,
    pub(super) count: &'a mut i32,
    pub(super) cmdline_buf: &'a mut Vec<u8>,
    pub(super) fgetline: &'a LineGetter,
}

/// How a statement left the run.
enum Flow {
    /// Go on with the next line.
    Next,
    /// `do_cmdline()` takes over after the line: it jumps or ends the loop, or
    /// the line left something for `do_cmdline()` to handle, such as an error.
    Stop,
}

thread_local! {
    /// Parsed lines by their text, for loops that start more than once.
    static LINES: RefCell<HashMap<Box<[u8]>, Parsed>> = RefCell::new(HashMap::new());

    /// Whether runs are on; `NERO_NO_PARSED_LINES` turns them off.
    static ENABLED: bool = std::env::var_os("NERO_NO_PARSED_LINES").is_none();
}

/// The state that a run saves and sets once, where `do_one_cmd()` does it for
/// each line.
struct Saved {
    level: i32,
    cmdmod: Cmdmod,
}

fn start(level: i32) -> Saved {
    let cmdmod = cmdline_state().take_cmdmod();
    cmdline_state().set_cmdmod(Cmdmod::new());
    exc_state().ex_nesting_level.set(level);
    Saved { level, cmdmod }
}

fn finish(saved: Saved) {
    exc_state().ex_nesting_level.set(saved.level - 1);
    cmdline_state().set_cmdmod(saved.cmdmod);
}

/// Runs `line` with the steps of `do_one_cmd()` around it, and returns how it
/// left the run.
fn step(line: &Line, cstack: &mut Box<Cstack>, fgetline: &LineGetter, level: i32) -> Flow {
    let save_reg_executing = input_state().reg_executing.get();
    let save_pending_end_reg_executing = input_state().pending_end_reg_executing.get();
    if DOCMD_STATE.with(|s| s.quitmore.get()) != 0 && !fgetline.is_func() && !fgetline.is_autocmd()
    {
        DOCMD_STATE.with(|s| s.quitmore.set(s.quitmore.get() - 1));
    }
    dbg_clear_skipped();
    let called_emsg = msg_state().called_emsg.get();

    let flow = exec(line, cstack);

    // do_one_cmd() after the command, which needs the condition stack in a
    // command for what it rarely has to do.
    if exc_state().need_rethrow.get() || DOCMD_STATE.with(|s| s.check_cstack.get()) {
        let mut ea = Exarg::new();
        ea.cstack = Some(std::mem::take(cstack));
        ea.ea_getline = fgetline.clone();
        after_command(&mut ea, fgetline);
        *cstack = ea.cstack.take().unwrap_or_default();
    }
    if curwin().w_cursor.get().lnum == 0 {
        curwin().w_cursor.set_lnum(1);
        curwin().w_cursor.set_col(0);
    }
    do_errthrow(Some(cstack), Some(CMDNAMES[line.cmdidx as usize].cmd_name));
    input_state().reg_executing.set(save_reg_executing);
    input_state()
        .pending_end_reg_executing
        .set(save_pending_end_reg_executing);

    // An error under ":silent!" counts in "called_emsg" only.
    let unusual = !quiet(level) || msg_state().called_emsg.get() != called_emsg;
    match flow {
        Flow::Next if unusual => Flow::Stop,
        flow => flow,
    }
}

/// The function or script `do_cmdline()` gets lines from, found once per run
/// instead of through its `LineGetter` for each line.
enum Origin {
    Func(Rc<crate::profile::Funccall>),
    Source(Rc<RefCell<crate::runtime::SourceCookie>>),
    Other,
}

impl Origin {
    fn of(fgetline: &LineGetter) -> Self {
        if let Some(fcp) = fgetline.as_func() {
            Self::Func(fcp)
        } else if let Some(sp) = fgetline.as_source() {
            Self::Source(sp)
        } else {
            Self::Other
        }
    }

    /// `func_has_ended()` or `source_finished()`.
    fn ended(&self) -> bool {
        match self {
            Self::Func(fcp) => func_has_ended(fcp) != 0,
            Self::Source(sp) => sp.borrow().finished(),
            Self::Other => false,
        }
    }

    /// `LineGetter::dbg_tick_get()`
    fn dbg_tick(&self) -> Option<i32> {
        match self {
            Self::Func(fcp) => Some(fcp.fc_dbg_tick.get()),
            Self::Source(sp) => Some(sp.borrow().dbg_tick),
            Self::Other => None,
        }
    }

    /// `LineGetter::breakpoint_get()`
    fn breakpoint(&self) -> Option<Linenr> {
        match self {
            Self::Func(fcp) => Some(fcp.fc_breakpoint.get()),
            Self::Source(sp) => Some(sp.borrow().breakpoint),
            Self::Other => None,
        }
    }
}

/// Runs the repeated loop lines from `*current_line` on, for which
/// `do_cmdline()` has taken the steps before `do_one_cmd()`. Returns `None`
/// when it runs none.
pub(super) fn run(mut r: Run<'_>) -> Option<Ran> {
    let level = exc_state().ex_nesting_level.get() + 1;
    if !ENABLED.with(|e| *e) || !quiet(level) {
        return None;
    }
    let (mut line, _) = line_at(r.cookie, *r.current_line)?;
    if !can_run(&line, r.cstack) {
        return None;
    }
    let origin = Origin::of(r.fgetline);
    let exline = r.fgetline.is_exline();
    let saved = start(level);

    let mut idx = *r.current_line;
    let ran = loop {
        let flow = step(&line, r.cstack, r.fgetline, level);
        let has_next = line.next.is_some();
        if !matches!(flow, Flow::Next) {
            r.cookie.borrow_mut().current_line = idx;
            leave_next(r.cmdline_buf, line.next.as_deref());
            break Ran::After { has_next };
        }

        // do_cmdline() after the line, when nothing unusual happened.
        if !has_next
            && exline
            && let Some(new_cmd) = cmdline_state().new_last_cmdline.take()
        {
            cmdline_state().last_cmdline.set(Some(new_cmd));
        }
        idx += 1;
        if exc_state().trylevel.get() == 0 {
            exc_state().force_abort.set(false);
        }
        match next_line(&mut r, &origin, idx) {
            Some(following) => line = following,
            None => {
                *r.current_line = idx;
                leave_next(r.cmdline_buf, line.next.as_deref());
                break Ran::Before { has_next };
            }
        }
    };

    finish(saved);
    Some(ran)
}

/// Puts `next` in the command line, as `do_one_cmd()` does.
fn leave_next(cmdline_buf: &mut Vec<u8>, next: Option<&[u8]>) {
    cmdline_buf.clear();
    if let Some(next) = next {
        cmdline_buf.extend_from_slice(next);
        cmdline_buf.push(0);
    }
}

/// Whether `do_cmdline()` and `do_one_cmd()` would take no steps for a line at
/// `ex_nesting_level` `level` beyond those a run takes.
fn quiet(level: i32) -> bool {
    exc_state().do_profiling.get() != PROF_YES
        && opt_vars().p_verbose.get() < 15
        && msg_state().did_emsg.get() == 0
        && !input_state().got_int.get()
        && !exc_state().did_throw.get()
        && !exc_state().force_abort.get()
        && !exc_state().need_rethrow.get()
        && !exc_state().exiting.get()
        && dbg_quiet(level)
}

/// `do_cmdline()` before repeated line `idx`: the parsed line when a run can
/// run it.
fn next_line(r: &mut Run<'_>, origin: &Origin, idx: i32) -> Option<Rc<Line>> {
    if origin.ended()
        || origin
            .dbg_tick()
            .is_some_and(|dt| dt != exc_state().debug_tick.get())
    {
        return None;
    }
    let (line, lnum) = line_at(r.cookie, idx)?;
    if origin.breakpoint().is_some_and(|bp| bp != 0 && bp <= lnum) || !can_run(&line, r.cstack) {
        return None;
    }
    set_sourcing_lnum(lnum);
    exc_state().did_endif.set(false);
    *r.count += 1;
    Some(line)
}

/// Repeated line `idx` of the loop, parsed, and its line number.
fn line_at(cookie: &RefCell<LoopCookie>, idx: i32) -> Option<(Rc<Line>, Linenr)> {
    let idx = usize::try_from(idx).ok()?;
    let text = {
        let cookie = cookie.borrow();
        let wcmd = cookie.lines.get(idx)?;
        if let Some(parsed) = wcmd.parsed.get() {
            return Some((parsed.clone()?, wcmd.lnum));
        }
        wcmd.line.clone()
    };
    let found = parsed(text.to_bytes());
    let cookie = cookie.borrow();
    let wcmd = cookie.lines.get(idx)?;
    let line = wcmd.parsed.get_or_init(|| found).clone()?;
    Some((line, wcmd.lnum))
}

/// `text` parsed, from `LINES` when it has been parsed before.
fn parsed(text: &[u8]) -> Parsed {
    LINES.with(|lines| {
        if let Some(found) = lines.borrow().get(text) {
            return found.clone();
        }
        let found = compile(text).map(Rc::new);
        let mut lines = lines.borrow_mut();
        if lines.len() >= 4096 {
            lines.clear();
        }
        lines.insert(text.into(), found.clone());
        found
    })
}

/// Parses line `text`, or returns `None` when a run does not run it.
fn compile(text: &[u8]) -> Option<Line> {
    let (cmdidx, name_end) = cmd_name(text)?;
    let after = &text[name_end..];
    // None of these commands takes a "!".
    if after.first() == Some(&b'!') {
        return None;
    }
    let arg = skipwhite_slice(after);
    let (stmt, next) = match cmdidx {
        Cmdidx::CmdLet => compile_let(arg)?,
        Cmdidx::CmdIf => {
            let (text, expr, next) = compile_cond(arg)?;
            (Stmt::If { text, expr }, next)
        }
        Cmdidx::CmdElseif => {
            let (text, expr, next) = compile_cond(arg)?;
            (Stmt::Elseif { text, expr }, next)
        }
        Cmdidx::CmdWhile => {
            let (text, expr, next) = compile_cond(arg)?;
            (Stmt::While { text, expr }, next)
        }
        // A repeating ":for" reads its list no more, nor sets a next command.
        Cmdidx::CmdFor if !arg.is_empty() => (Stmt::For { arg: arg.into() }, None),
        Cmdidx::CmdElse => (Stmt::Else, bar_next(arg)?),
        Cmdidx::CmdEndif => (Stmt::Endif, bar_next(arg)?),
        Cmdidx::CmdEndfor => (Stmt::EndLoop { csf: CSF_FOR }, bar_next(arg)?),
        Cmdidx::CmdEndwhile => (Stmt::EndLoop { csf: CSF_WHILE }, bar_next(arg)?),
        Cmdidx::CmdContinue => (Stmt::Continue, bar_next(arg)?),
        Cmdidx::CmdBreak => (Stmt::Break, bar_next(arg)?),
        _ => return None,
    };
    if CMDNAMES[cmdidx as usize].cmd_argt != stmt.flags() {
        return None;
    }
    Some(Line { stmt, cmdidx, next })
}

/// The command of `line` and the offset after its name, when the line has no
/// modifiers and no range and names a built-in command, so that what
/// `parse_command_modifiers()` and `find_excmd_after_range()` find in
/// `do_one_cmd()` depends on the text alone.
fn cmd_name(line: &[u8]) -> Option<(Cmdidx, usize)> {
    let start = line
        .iter()
        .take_while(|&&b| matches!(b, b' ' | b'\t' | b':'))
        .count();
    let cmd = &line[start..];
    // A range starts with something else, and a modifier needs parsing.
    if !cmd.first().is_some_and(u8::is_ascii_lowercase) || modifier_len(cmd) != 0 {
        return None;
    }
    let mut ea = Exarg::new();
    ea.cmdline_buf = [line, b"\0"].concat();
    ea.cmd = ea.cmdline_ptr();
    let mut errormsg = None;
    let mut cmod = Cmdmod::new();
    if !parse_command_modifiers(&mut ea, &mut errormsg, &mut cmod, false)
        || ea.cmd.offset() != start
        || cmod.cmod_flags != 0
        || cmod.cmod_split != 0
        || cmod.cmod_tab != 0
        || cmod.cmod_verbose != 0
        || cmod.cmod_filter_pat.is_some()
    {
        return None;
    }
    let p = find_excmd_after_range(&mut ea);
    if p.is_null() || ea.flags != 0 || !ea.cmdidx.is_builtin() {
        return None;
    }
    Some((ea.cmdidx, start + p.offset_from(&ea.cmd)))
}

fn compile_let(arg: &[u8]) -> Option<(Stmt, NextCmd)> {
    let mut var_count = 0;
    let mut semicolon = 0;
    let after = skip_var_list(arg, &mut var_count, &mut semicolon, true)?;
    if !is_simple_var(&arg[..arg.len() - after.len()]) {
        return None;
    }
    let rest = skipwhite_slice(after);
    let (op, len): (&'static CStr, usize) = match rest {
        [b'=', b'<', b'<', ..] => return None,
        [b'=', ..] => (c"=", 1),
        [b'.', b'.', b'=', ..] => (c".", 3),
        [b'+', b'=', ..] => (c"+", 2),
        [b'-', b'=', ..] => (c"-", 2),
        [b'*', b'=', ..] => (c"*", 2),
        [b'/', b'=', ..] => (c"/", 2),
        [b'%', b'=', ..] => (c"%", 2),
        [b'.', b'=', ..] => (c".", 2),
        _ => return None,
    };
    let text = skipwhite_slice(&rest[len..]);
    let (expr, next) = compile_expr(text)?;
    Some((
        Stmt::Let {
            arg: arg.into(),
            op,
            text: text.into(),
            expr,
        },
        next,
    ))
}

/// The condition of `:if`, `:elseif` and `:while`.
fn compile_cond(arg: &[u8]) -> Option<(Box<[u8]>, Expr, NextCmd)> {
    if matches!(arg.first(), None | Some(b'|' | b'\n')) {
        return None;
    }
    let (expr, next) = compile_expr(arg)?;
    Some((arg.into(), expr, next))
}

/// An expression that ends the command, as `eval0()` reads it, and the next
/// command it leaves.
fn compile_expr(text: &[u8]) -> Option<(Expr, NextCmd)> {
    let (expr, len) = Expr::parse(text)?;
    let rest = &text[len..];
    if ends_excmd(i32::from(rest.first().copied().unwrap_or(0))) == 0 {
        return None;
    }
    Some((expr, nonempty(check_nextcmd(rest))))
}

/// The next command `separate_nextcmd()` finds after a command that takes no
/// argument, when it needs nothing but `arg`.
fn bar_next(arg: &[u8]) -> Option<NextCmd> {
    match arg.first() {
        None | Some(b'"') => Some(None),
        Some(b'|') => Some(nonempty(Some(&arg[1..]))),
        _ => None,
    }
}

fn nonempty(next: Option<&[u8]>) -> NextCmd {
    next.filter(|n| !n.is_empty()).map(Box::from)
}

/// Whether a run can run `line` now, which is when `do_one_cmd()` would give
/// no error message of its own for it.
fn can_run(line: &Line, cstack: &Cstack) -> bool {
    let top = cstack.cs_idx;
    let flags = |i: i32| cstack.cs_flags[i as usize];
    let in_if = top >= 0 && flags(top) & (CSF_WHILE | CSF_FOR | CSF_TRY) == 0;
    match &line.stmt {
        Stmt::Let { expr, .. } => expr.fits(),
        Stmt::If { expr, .. } => top != CSTACK_LEN as i32 - 1 && expr.fits(),
        Stmt::Elseif { expr, .. } => in_if && flags(top) & CSF_ELSE == 0 && expr.fits(),
        Stmt::Else => in_if && flags(top) & CSF_ELSE == 0,
        Stmt::Endif => in_if,
        Stmt::While { expr, .. } => repeating(cstack, CSF_WHILE) && expr.fits(),
        Stmt::For { .. } => repeating(cstack, CSF_FOR),
        Stmt::EndLoop { csf } => cstack.cs_looplevel > 0 && top >= 0 && flags(top) & csf != 0,
        Stmt::Continue | Stmt::Break => {
            skipping(cstack) || (cstack.cs_looplevel > 0 && loop_without_try(cstack))
        }
    }
}

/// Whether the top of `cstack` is a `csf` loop about to repeat.
fn repeating(cstack: &Cstack, csf: i32) -> bool {
    let top = cstack.cs_idx;
    cstack.cs_lflags & CSL_HAD_LOOP != 0
        && top >= 0
        && top != CSTACK_LEN as i32 - 1
        && cstack.cs_flags[top as usize] & csf != 0
}

/// Whether the innermost loop has no `:try` in it.
fn loop_without_try(cstack: &Cstack) -> bool {
    for i in (0..=cstack.cs_idx).rev() {
        let flags = cstack.cs_flags[i as usize];
        if flags & (CSF_WHILE | CSF_FOR) != 0 {
            return true;
        }
        if flags & CSF_TRY != 0 {
            return false;
        }
    }
    false
}

/// `ea.skip` of `do_one_cmd()` when nothing unusual is going on.
fn skipping(cstack: &Cstack) -> bool {
    cstack.cs_idx >= 0 && cstack.cs_flags[cstack.cs_idx as usize] & CSF_ACTIVE == 0
}

/// Runs `line`, as its command does when `can_run()`.
fn exec(line: &Line, cstack: &mut Cstack) -> Flow {
    match &line.stmt {
        // ex_let()
        Stmt::Let {
            arg,
            op,
            text,
            expr,
        } => {
            if !skipping(cstack)
                && let Ok(mut rettv) = expr.eval0(text)
            {
                ex_let_vars(arg, &mut rettv, 0, 0, 0, 0, Some(op));
                tv_clear(&mut rettv);
            }
            Flow::Next
        }
        // ex_if()
        Stmt::If { text, expr } => {
            cstack.cs_idx += 1;
            let idx = cstack.cs_idx as usize;
            cstack.cs_flags[idx] = 0;
            let cond = Cond::eval(expr, text, check_skip(cstack));
            cstack.cs_flags[idx] = cond.branch_flags();
            Flow::Next
        }
        // ex_else()
        Stmt::Elseif { text, expr } => {
            let idx = cstack.cs_idx as usize;
            let skip = start_else(cstack, idx);
            cstack.cs_flags[idx] = Cond::eval(expr, text, skip).branch_flags();
            Flow::Next
        }
        Stmt::Else => {
            let idx = cstack.cs_idx as usize;
            start_else(cstack, idx);
            cstack.cs_flags[idx] |= CSF_ELSE;
            Flow::Next
        }
        // ex_endif()
        Stmt::Endif => {
            exc_state().did_endif.set(true);
            cstack.cs_idx -= 1;
            Flow::Next
        }
        // ex_while()
        Stmt::While { text, expr } => {
            let idx = cstack.cs_idx as usize;
            cstack.cs_flags[idx] = CSF_WHILE;
            let cond = Cond::eval(expr, text, check_skip(cstack));
            repeat(cstack, idx, cond)
        }
        Stmt::For { arg } => {
            let idx = cstack.cs_idx as usize;
            cstack.cs_flags[idx] = CSF_FOR;
            let cond = if check_skip(cstack) {
                Cond::Skipped
            } else {
                Cond::Value(
                    cstack.cs_forinfo[idx]
                        .as_deref_mut()
                        .is_some_and(|fi| next_for_item(fi, arg)),
                )
            };
            if !matches!(cond, Cond::Value(true)) {
                cstack.cs_forinfo[idx] = None;
            }
            repeat(cstack, idx, cond)
        }
        // ex_endwhile()
        Stmt::EndLoop { .. } => {
            cstack.cs_lflags |= CSL_HAD_ENDLOOP;
            Flow::Stop
        }
        // ex_continue() and ex_break(), skipped like other commands.
        Stmt::Continue => {
            if skipping(cstack) {
                return Flow::Next;
            }
            let idx = cleanup_conditionals(cstack, CSF_WHILE | CSF_FOR, false);
            let mut trylevel = cstack.cs_trylevel;
            rewind_conditionals(cstack, idx, CSF_TRY, &mut trylevel);
            cstack.cs_trylevel = trylevel;
            cstack.cs_lflags |= CSL_HAD_CONT;
            Flow::Stop
        }
        Stmt::Break => {
            if skipping(cstack) {
                return Flow::Next;
            }
            cleanup_conditionals(cstack, CSF_WHILE | CSF_FOR, true);
            Flow::Stop
        }
    }
}

/// The start of `:else` and `:elseif` in `ex_else()`. Returns whether the
/// branch is skipped.
fn start_else(cstack: &mut Cstack, idx: usize) -> bool {
    if check_skip(cstack) || cstack.cs_flags[idx] & CSF_TRUE != 0 {
        cstack.cs_flags[idx] = CSF_TRUE;
        true
    } else {
        cstack.cs_flags[idx] = CSF_ACTIVE;
        false
    }
}

/// The end of `ex_while()` for a repeating loop.
fn repeat(cstack: &mut Cstack, idx: usize, cond: Cond) -> Flow {
    if let Cond::Value(true) = cond {
        cstack.cs_flags[idx] |= CSF_ACTIVE | CSF_TRUE;
        cstack.cs_lflags ^= CSL_HAD_LOOP;
        Flow::Next
    } else {
        cstack.cs_lflags &= !CSL_HAD_LOOP;
        if let Cond::Value(false) = cond {
            cstack.cs_flags[idx] |= CSF_TRUE;
        }
        Flow::Stop
    }
}

/// The condition of `:if`, `:elseif` or `:while`, as `eval_to_bool()` gives it.
#[derive(Clone, Copy)]
enum Cond {
    /// The command is skipped, so the condition is not evaluated.
    Skipped,
    /// Evaluating it gave an error.
    Failed,
    Value(bool),
}

impl Cond {
    /// `eval_to_bool()` of `text`, parsed as `expr`, unless `skip`.
    fn eval(expr: &Expr, text: &[u8], skip: bool) -> Self {
        if skip {
            return Self::Skipped;
        }
        let Ok(mut tv) = expr.eval0(text) else {
            return Self::Failed;
        };
        let mut error = false;
        let value = tv_get_number_chk(&tv, Some(&mut error)) != 0;
        tv_clear(&mut tv);
        if error {
            Self::Failed
        } else {
            Self::Value(value)
        }
    }

    /// The flags `ex_if()` and `ex_else()` give a branch with this condition.
    fn branch_flags(self) -> i32 {
        match self {
            Self::Value(true) => CSF_ACTIVE | CSF_TRUE,
            Self::Value(false) => 0,
            // So that the :if never gets active.
            Self::Skipped | Self::Failed => CSF_TRUE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fails when a command a run runs no longer has the flags it needs, for
    /// which `compile()` leaves its lines to `do_one_cmd()`.
    #[test]
    fn test_compiles_each_statement() {
        let _guard = crate::globals::TEST_GLOBAL_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _editor = crate::main_init::test_editor();
        for line in [
            "let x = 1",
            "let x += 1",
            "if x",
            "elseif x",
            "else",
            "endif",
            "while x",
            "for x in y",
            "endwhile",
            "endfor",
            "continue",
            "break",
        ] {
            assert!(compile(line.as_bytes()).is_some(), "{line}");
        }
    }
}
