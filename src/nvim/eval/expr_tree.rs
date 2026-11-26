//! Expressions parsed once into a tree, for command lines that run again and
//! again (`ex_docmd::parsed_lines`). Each node takes the steps of the part of
//! `eval1()` to `eval7()` that reads it, with the same helpers, so a tree gives
//! the value and the errors that `eval0()` gives for its text.
//!
//! Only part of the syntax has a tree: number, float and string literals,
//! variables by name, `!`, `-` and `+` before a value, `?:`, `??`, `||`, `&&`,
//! comparisons, `+`, `-`, `.`, `..`, `*`, `/`, `%` and parentheses.
//! `Expr::parse()` returns `None` for anything else, and the caller evaluates
//! the text with `eval0()`.
#![forbid(unsafe_code)]

use std::cell::Cell;

use super::*;
use crate::charset::skipwhite_slice;

/// Deepest nesting of `(` and `?:` parsed; deeper expressions use `eval0()`.
const MAX_DEPTH: usize = 50;

/// An expression parsed once.
pub(crate) struct Expr {
    /// Most `eval7()` calls `eval1()` has active at once for this text.
    eval7_depth: i32,
    body: Node,
}

/// The evaluation failed, and the error has been given.
#[derive(Debug)]
pub(crate) struct Failed;

type Eval = Result<(), Failed>;

enum Node {
    /// A number, float or string literal.
    Literal(TypVal),
    /// A variable read with `eval_variable()`.
    Var(Box<[u8]>),
    /// `!`, `-` and `+` before a value; `number` when the value is a number
    /// literal, which takes a `-` or `+` before any `!`.
    Leader {
        leader: Box<[u8]>,
        number: bool,
        value: Box<Node>,
    },
    /// `cond ? then : other`
    Cond(Box<Node>, Box<Node>, Box<Node>),
    /// `value ?? other`
    Falsy(Box<Node>, Box<Node>),
    Or(Vec<Node>),
    And(Vec<Node>),
    Compare {
        ty: Exprtype,
        /// `None`: compare with 'ignorecase'.
        ic: Option<bool>,
        lhs: Box<Node>,
        rhs: Box<Node>,
    },
    /// `+`, `-`, `.` and `..`, left to right.
    AddSub(Box<Node>, Vec<(u8, Node)>),
    /// `*`, `/` and `%`, left to right.
    MulDiv(Box<Node>, Vec<(u8, Node)>),
}

impl Expr {
    /// Parses the expression at the start of `text` as `eval1()` reads it.
    /// Returns it with the length of the text `eval1()` reads, white space
    /// after it included, or `None` when it uses anything without a tree.
    pub(crate) fn parse(text: &[u8]) -> Option<(Self, usize)> {
        let mut p = skipwhite_slice(text);
        let parser = Parser {
            depth: Cell::new(0),
            eval7: Cell::new(0),
            max_eval7: Cell::new(0),
        };
        let body = parser.expr1(&mut p)?;
        let expr = Self {
            eval7_depth: parser.max_eval7.get(),
            body,
        };
        Some((expr, text.len() - p.len()))
    }

    /// Whether `eval1()` would read this expression now without "E1169:
    /// Expression too recursive".
    pub(crate) fn fits(&self) -> bool {
        EVAL_STATE.with(|s| s.eval7_recurse.get()) + self.eval7_depth <= MAX_EVAL7_RECURSE
    }

    /// The value `eval0()` gives for `text`, which this expression was parsed
    /// from and which ends where the command ends. The expression must fit.
    pub(crate) fn eval0(&self, text: &[u8]) -> Result<TypVal, Failed> {
        let did_emsg_before = msg_state().did_emsg.get();
        let called_emsg_before = msg_state().called_emsg.get();
        let mut rettv = TypVal::new();
        if eval(&self.body, &mut rettv).is_ok() {
            return Ok(rettv);
        }
        if !aborting()
            && msg_state().did_emsg.get() == did_emsg_before
            && msg_state().called_emsg.get() == called_emsg_before
        {
            semsg!(
                gettext(E_INVEXPR2),
                bytes_to_cstring(crate::memory::bytes_until_nul(text)).as_c_str()
            );
        }
        Err(Failed)
    }
}

/// Reads text into nodes, following the levels of `eval1()` to `eval7()`.
struct Parser {
    depth: Cell<usize>,
    /// `eval7()` calls active at the current position.
    eval7: Cell<i32>,
    max_eval7: Cell<i32>,
}

impl Parser {
    fn expr1(&self, p: &mut &[u8]) -> Option<Node> {
        let depth = self.depth.get();
        if depth >= MAX_DEPTH {
            return None;
        }
        self.depth.set(depth + 1);
        let node = self.cond(p);
        self.depth.set(depth);
        node
    }

    fn cond(&self, p: &mut &[u8]) -> Option<Node> {
        let cond = self.expr2(p)?;
        if p.first() != Some(&b'?') {
            return Some(cond);
        }
        let falsy = p.get(1) == Some(&b'?');
        *p = skipwhite_slice(&p[if falsy { 2 } else { 1 }..]);
        let then = self.expr1(p)?;
        if falsy {
            return Some(Node::Falsy(Box::new(cond), Box::new(then)));
        }
        if p.first() != Some(&b':') {
            return None;
        }
        *p = skipwhite_slice(&p[1..]);
        let other = self.expr1(p)?;
        Some(Node::Cond(Box::new(cond), Box::new(then), Box::new(other)))
    }

    fn expr2(&self, p: &mut &[u8]) -> Option<Node> {
        let first = self.expr3(p)?;
        if !p.starts_with(b"||") {
            return Some(first);
        }
        let mut items = vec![first];
        while p.starts_with(b"||") {
            *p = skipwhite_slice(&p[2..]);
            items.push(self.expr3(p)?);
        }
        Some(Node::Or(items))
    }

    fn expr3(&self, p: &mut &[u8]) -> Option<Node> {
        let first = self.expr4(p)?;
        if !p.starts_with(b"&&") {
            return Some(first);
        }
        let mut items = vec![first];
        while p.starts_with(b"&&") {
            *p = skipwhite_slice(&p[2..]);
            items.push(self.expr4(p)?);
        }
        Some(Node::And(items))
    }

    fn expr4(&self, p: &mut &[u8]) -> Option<Node> {
        let lhs = self.expr5(p)?;
        let mut len = 2;
        let ty = get_compare_type(p, &mut len);
        if ty == EXPR_UNKNOWN {
            return Some(lhs);
        }
        let ic = match p.get(len) {
            Some(b'?') => Some(true),
            Some(b'#') => Some(false),
            _ => None,
        };
        if ic.is_some() {
            len += 1;
        }
        *p = skipwhite_slice(&p[len..]);
        let rhs = self.expr5(p)?;
        Some(Node::Compare {
            ty,
            ic,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        })
    }

    fn expr5(&self, p: &mut &[u8]) -> Option<Node> {
        let first = self.expr6(p, false)?;
        let mut rest = Vec::new();
        while let Some(&op @ (b'+' | b'-' | b'.')) = p.first() {
            let len = if op == b'.' && p.get(1) == Some(&b'.') {
                2
            } else {
                1
            };
            *p = skipwhite_slice(&p[len..]);
            rest.push((op, self.expr6(p, op == b'.')?));
        }
        Some(if rest.is_empty() {
            first
        } else {
            Node::AddSub(Box::new(first), rest)
        })
    }

    fn expr6(&self, p: &mut &[u8], want_string: bool) -> Option<Node> {
        let first = self.expr7(p, want_string)?;
        let mut rest = Vec::new();
        while let Some(&op @ (b'*' | b'/' | b'%')) = p.first() {
            *p = skipwhite_slice(&p[1..]);
            rest.push((op, self.expr7(p, false)?));
        }
        Some(if rest.is_empty() {
            first
        } else {
            Node::MulDiv(Box::new(first), rest)
        })
    }

    fn expr7(&self, p: &mut &[u8], want_string: bool) -> Option<Node> {
        let level = self.eval7.get() + 1;
        self.eval7.set(level);
        self.max_eval7.set(self.max_eval7.get().max(level));
        let node = self.atom(p, want_string);
        self.eval7.set(level - 1);
        node
    }

    /// A value with the `!`, `-` and `+` before it, as `eval7()` reads it.
    fn atom(&self, p: &mut &[u8], want_string: bool) -> Option<Node> {
        let start = *p;
        let mut cur = start;
        while matches!(cur.first(), Some(b'!' | b'-' | b'+')) {
            cur = skipwhite_slice(&cur[1..]);
        }
        let leader = &start[..start.len() - cur.len()];
        let mut number = false;
        let value = match *cur.first()? {
            b'0'..=b'9' => {
                // A Blob literal makes a new Blob each time it is read.
                if matches!(cur.get(1), Some(b'z' | b'Z')) {
                    return None;
                }
                number = true;
                literal(&mut cur, |q, tv| eval_number(q, tv, true, want_string))?
            }
            b'\'' => literal(&mut cur, |q, tv| eval_lit_string(q, tv, true, false))?,
            b'"' => {
                // "\<SID>" depends on the script that reads it.
                if has_key_notation(cur) {
                    return None;
                }
                literal(&mut cur, |q, tv| eval_string(q, tv, true, false))?
            }
            b'(' => {
                cur = skipwhite_slice(&cur[1..]);
                let inner = self.expr1(&mut cur)?;
                if cur.first() != Some(&b')') {
                    return None;
                }
                cur = &cur[1..];
                // "(dict).key" reads a member.
                if cur.first() == Some(&b'.') {
                    return None;
                }
                inner
            }
            _ => {
                let mut rest = cur;
                let mut alias = None;
                let len = get_name_len(&mut rest, &mut alias, false, false);
                let Ok(len) = usize::try_from(len) else {
                    return None;
                };
                let name = &cur[..len];
                // "dict.key" reads a member, and "name(" or "name (" calls
                // a function; `rest` is past the white space after the name.
                // After a space, "." joins strings, or for a v:lua value
                // fails as it does here.
                if len == 0
                    || alias.is_some()
                    || !is_simple_var(name)
                    || cur.get(len) == Some(&b'.')
                    || rest.first() == Some(&b'(')
                {
                    return None;
                }
                cur = rest;
                Node::Var(name.into())
            }
        };
        cur = skipwhite_slice(cur);
        if matches!(cur.first(), Some(b'[' | b'(')) || cur.starts_with(b"->") {
            return None;
        }
        *p = cur;
        Some(if leader.is_empty() {
            value
        } else {
            Node::Leader {
                leader: leader.into(),
                number,
                value: Box::new(value),
            }
        })
    }
}

/// A variable name, with or without a scope, that needs no autoload script
/// and no curly braces.
pub(crate) fn is_simple_var(name: &[u8]) -> bool {
    let rest = match name {
        [
            b'g' | b'b' | b'w' | b't' | b's' | b'l' | b'a' | b'v',
            b':',
            rest @ ..,
        ] => rest,
        _ => name,
    };
    !rest.is_empty() && rest.iter().all(|&c| c.is_ascii_alphanumeric() || c == b'_')
}

/// Whether the `"` string at the start of `s` has a `\<` key name.
fn has_key_notation(s: &[u8]) -> bool {
    let mut i = 1;
    while i < s.len() && s[i] != b'"' {
        if s[i] == b'\\' {
            if s.get(i + 1) == Some(&b'<') {
                return true;
            }
            i += 1;
        }
        i += 1;
    }
    false
}

/// The literal at the start of `p`, read by `read`.
fn literal(p: &mut &[u8], read: impl FnOnce(&mut &[u8], &mut TypVal) -> bool) -> Option<Node> {
    let mut tv = tv_initial_value();
    let mut q = *p;
    if !read(&mut q, &mut tv) {
        return None;
    }
    *p = q;
    Some(Node::Literal(tv))
}

fn check(ok: bool) -> Eval {
    if ok { Ok(()) } else { Err(Failed) }
}

/// Evaluates `node` into `rettv`, leaving in `rettv` what the matching part
/// of `eval1()` to `eval7()` leaves when it fails.
fn eval(node: &Node, rettv: &mut TypVal) -> Eval {
    match node {
        Node::Literal(tv) => {
            rettv.value = tv.value.clone();
            Ok(())
        }
        // eval7() of a name, then handle_subscript(), which fails without a
        // message for a v:lua value that no ".name(" follows.
        Node::Var(name) => {
            rettv.value = TvValue::Unknown;
            check(eval_variable(
                name,
                name.len() as i32,
                Some(rettv),
                None,
                true,
                false,
            ))?;
            if tv_is_luafunc(rettv) {
                tv_clear(rettv);
                return Err(Failed);
            }
            Ok(())
        }
        Node::Leader {
            leader,
            number,
            value,
        } => {
            eval(value, rettv)?;
            let mut len = leader.len();
            if *number {
                check(eval7_leader(rettv, true, leader, &mut len))?;
            }
            if len > 0 {
                check(eval7_leader(rettv, false, leader, &mut len))?;
            }
            Ok(())
        }
        // eval1()
        Node::Cond(cond, then, other) => {
            eval(cond, rettv)?;
            let mut error = false;
            let result = tv_get_number_chk(rettv, Some(&mut error)) != 0;
            tv_clear(rettv);
            check(!error)?;
            if !result {
                // What the skipped branch leaves.
                *rettv = TypVal::new();
            }
            let mut var2 = tv_initial_value();
            eval(if result { then } else { other }, &mut var2)?;
            *rettv = var2;
            Ok(())
        }
        Node::Falsy(lhs, rhs) => {
            eval(lhs, rettv)?;
            if tv2bool(rettv) {
                return Ok(());
            }
            tv_clear(rettv);
            let mut var2 = tv_initial_value();
            eval(rhs, &mut var2)?;
            *rettv = var2;
            Ok(())
        }
        Node::Or(items) => logical(items, Logic::Or, rettv),
        Node::And(items) => logical(items, Logic::And, rettv),
        // eval4()
        Node::Compare { ty, ic, lhs, rhs } => {
            eval(lhs, rettv)?;
            let ic = ic.unwrap_or_else(|| opt_vars().p_ic.get());
            let mut var2 = tv_initial_value();
            if let Err(failed) = eval(rhs, &mut var2) {
                tv_clear(rettv);
                return Err(failed);
            }
            let ret = typval_compare(rettv, &mut var2, *ty, ic);
            tv_clear(&mut var2);
            check(ret)
        }
        Node::AddSub(first, rest) => {
            eval(first, rettv)?;
            for (op, node) in rest {
                add_sub(i32::from(*op), node, rettv)?;
            }
            Ok(())
        }
        // eval6()
        Node::MulDiv(first, rest) => {
            eval(first, rettv)?;
            for (op, node) in rest {
                let mut var2 = tv_initial_value();
                eval(node, &mut var2)?;
                check(eval_multdiv_number(rettv, &mut var2, i32::from(*op)))?;
            }
            Ok(())
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Logic {
    Or,
    And,
}

/// `||` or `&&`, as in `eval2()` and `eval3()`.
fn logical(items: &[Node], op: Logic, rettv: &mut TypVal) -> Eval {
    // The value of an item that decides the result, so the rest are skipped.
    let decides = op == Logic::Or;
    let (first, rest) = items.split_first().ok_or(Failed)?;
    eval(first, rettv)?;
    let mut error = false;
    let mut result = tv_get_number_chk(rettv, Some(&mut error)) != 0;
    tv_clear(rettv);
    check(!error)?;
    for item in rest {
        if result != decides {
            let mut var2 = tv_initial_value();
            eval(item, &mut var2)?;
            let mut error = false;
            result = tv_get_number_chk(&var2, Some(&mut error)) != 0;
            tv_clear(&mut var2);
            check(!error)?;
        }
        rettv.value = TvValue::Number(Varnumber::from(result));
    }
    Ok(())
}

/// One `+`, `-`, `.` or `..` step of `eval5()`.
fn add_sub(op: i32, node: &Node, rettv: &mut TypVal) -> Eval {
    let concat = op == i32::from(b'.');
    let plus = op == i32::from(b'+');
    let ty = rettv.var_type();
    if (!plus || (ty != VarType::VarList && ty != VarType::VarBlob))
        && (concat || ty != VarType::VarFloat)
        && ((concat && !tv_check_str(rettv)) || (!concat && !tv_check_num(rettv)))
    {
        tv_clear(rettv);
        return Err(Failed);
    }
    let mut var2 = tv_initial_value();
    if let Err(failed) = eval(node, &mut var2) {
        tv_clear(rettv);
        return Err(failed);
    }
    if concat {
        check(eval_concat_str(rettv, &mut var2))?;
    } else if plus && rettv.var_type() == VarType::VarBlob && var2.var_type() == VarType::VarBlob {
        eval_addblob(rettv, &mut var2);
    } else if plus && rettv.var_type() == VarType::VarList && var2.var_type() == VarType::VarList {
        check(eval_addlist(rettv, &mut var2))?;
    } else {
        check(eval_addsub_number(rettv, &mut var2, op))?;
    }
    tv_clear(&mut var2);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::encode::encode_tv2string;
    use crate::eval::vars::{VV_ERRMSG, get_vim_var_str, set_vim_var_string};

    /// Expressions with a tree, with values and errors.
    const CASES: &[&[u8]] = &[
        b"1 + 2",
        b"7 - g:n",
        b"g:n * 3",
        b"g:n / 2",
        b"g:n % 3",
        b"g:n / 0",
        b"-g:n",
        b"!g:n",
        b"!!g:s",
        b"- -1",
        b"+'3'",
        b"-'0x10'",
        b"1.5 * 2",
        b"g:f + 1",
        b"g:f % 2",
        b"1.0 / 0",
        b"-1.0 / 0",
        b"0.0 / 0",
        b"1.5e3 - 1",
        b"'a' . 'b'",
        b"'a' .. 1",
        b"g:s . g:n",
        b"g:s .. g:f",
        b"\"x\\ty\"",
        b"'it''s'",
        b"0x10 + 0b101 + 0o17 + 017",
        b"1 == 1",
        b"'a' ==? 'A'",
        b"'a' ==# 'A'",
        b"'a' == 'A'",
        b"'abc' =~ 'b'",
        b"'abc' !~# 'B'",
        b"g:n > 3 && g:n < 10",
        b"0 || ''",
        b"g:n ? 'y' : 'n'",
        b"0 ? 1 : 2",
        b"0 ? g:nope : 2",
        b"1 ? 2 : g:nope",
        b"'' ?? 'default'",
        b"g:n ?? g:nope",
        b"(1 + 2) * 3",
        b"((g:n))",
        b"g:l + g:l",
        b"g:l is g:l",
        b"g:l isnot g:l",
        b"g:d == g:d",
        b"g:s < 'b'",
        b"v:true",
        b"v:null",
        b"v:true + 1",
        b"g:n .. ''",
        b"count",
        b"1 + 2 + 3 - 4 * 5 / 6 % 7",
        b"g:nope",
        b"g:nope + 1",
        b"1 + g:nope",
        b"g:l - 1",
        b"g:l . 'a'",
        b"g:d + 1",
        b"g:l * g:nope",
        b"g:l ? 1 : 2",
        b"g:l || 1",
        b"1 && g:l",
        b"'a' + g:l",
        b"g:d . 1",
        b"v:lua",
        b"g:l > 1",
        b"g:d =~ 'a'",
        b"g:F == 1",
        b"1 . g:l",
        b"g:F . 'x'",
        b"g:l + 1",
    ];

    /// The value as `string()` gives it when there is one, and `v:errmsg`.
    fn outcome(result: Result<TypVal, Failed>) -> (Option<Vec<u8>>, Vec<u8>) {
        let errmsg = get_vim_var_str(VV_ERRMSG).to_bytes().to_vec();
        let value = result.ok().map(|mut tv| {
            let text = encode_tv2string(&mut tv, None).into_bytes();
            tv_clear(&mut tv);
            text
        });
        (value, errmsg)
    }

    fn plain_eval0(text: &[u8]) -> Result<TypVal, Failed> {
        let mut rettv = tv_initial_value();
        let mut evalarg = Evalarg {
            eval_flags: EVAL_EVALUATE,
        };
        if eval0(text, &mut rettv, None, &mut evalarg) {
            Ok(rettv)
        } else {
            Err(Failed)
        }
    }

    fn reset_errors() {
        set_vim_var_string(VV_ERRMSG, Some(c""), -1);
        msg_state().did_emsg.set(0);
    }

    #[test]
    fn test_trees_match_eval0() {
        let _editor = crate::main_init::test_editor();
        for cmd in [
            &b"let g:n = 7"[..],
            b"let g:s = 'abc'",
            b"let g:l = [1, 2]",
            b"let g:f = 1.5",
            b"let g:d = {'k': 1}",
            b"let g:F = function('len')",
        ] {
            assert!(crate::ex_docmd::do_cmdline_cmd(cmd));
        }

        for &text in CASES {
            let show = String::from_utf8_lossy(text);
            let (expr, len) = Expr::parse(text).unwrap_or_else(|| panic!("no tree for {show}"));
            assert_eq!(len, text.len(), "{show}");
            assert!(expr.fits(), "{show}");
            reset_errors();
            let tree = outcome(expr.eval0(text));
            reset_errors();
            let plain = outcome(plain_eval0(text));
            assert_eq!(tree, plain, "{show}");
        }
        reset_errors();
    }

    #[test]
    fn test_parse_stops_where_eval1_does() {
        let _editor = crate::main_init::test_editor();
        for (text, rest) in [
            (&b"1 + 2 | echo"[..], &b"| echo"[..]),
            (b"  g:n  \" comment", b"\" comment"),
            (b"x == y\n", b"\n"),
            (b"a ? b : c", b""),
        ] {
            let (_, len) = Expr::parse(text).expect("tree");
            assert_eq!(&text[len..], rest, "{}", String::from_utf8_lossy(text));
        }
    }

    #[test]
    fn test_parse_leaves_other_syntax_to_eval0() {
        let _editor = crate::main_init::test_editor();
        for text in [
            &b"len(g:l)"[..],
            b"g:l[0]",
            b"g:d.k",
            b"(g:d).k",
            b"[1]",
            b"{}",
            b"#{a: 1}",
            b"g:l->len()",
            b"0z12",
            b"\"\\<C-R>\"",
            b"@a",
            b"&ts",
            b"$HOME",
            b"{x -> x}",
            b"F (1)",
            b"auto#load",
            b"s:{'x'}",
            b"<SID>x",
            b"1 ? 2",
            b"(1",
        ] {
            assert!(
                Expr::parse(text).is_none(),
                "{}",
                String::from_utf8_lossy(text)
            );
        }
    }
}
