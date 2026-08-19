//! Translated from `src/nvim/register.c` (tractable core only).
//!
//! `register.c` (2867 lines) is the yank/delete/put register
//! subsystem: register storage, `"ay`/`"ap`-style register selection,
//! clipboard (`'clipboard'`) integration, and ShaDa persistence of
//! register contents - none of the actual yank/delete/put *commands*
//! (`op_yank`/`op_delete`/`do_put`, `ops.c`) are translated yet, so
//! nothing in this crate ever WRITES a real value into a register.
//!
//! Translated: the register-name/index plumbing (`op_reg_index`,
//! `valid_yank_reg`), the register storage array itself (`Y_REGS`,
//! `y_previous` as `Y_PREVIOUS`) and [`get_yank_register`]/
//! [`get_y_previous`], the
//! `"="`-register expression-source/result state ([`get_expr_line`]/
//! [`get_expr_line_src`]/[`set_expr_line`]), [`get_reg_contents`]/
//! `get_spec_reg` (`@r` in expressions - `eval7`'s own real caller),
//! `put_reedit_in_typebuf`/`put_in_typebuf`/
//! `execreg_line_continuation`/[`do_execreg`] (the register execution
//! queue builder), [`do_record`] (macro recording start/stop), and
//! [`insert_reg`] (register-to-stuff-buffer
//! insertion),
//! and `buffer.c`'s own `getaltfname` (`@#`) - now tractable IN FULL
//! (not just its own always-`None`-today fast path) now that
//! `buffer.rs`'s `buflist_findnr`/`buflist_name_nr` both exist.
//! `get_clipboard` always returns `false` (no provider registered) -
//! this crate has no clipboard-provider integration translated yet
//! (`ui_client.c`/Lua `provider#clipboard#`, a separate, substantial
//! undertaking); every OTHER real code path in [`get_yank_register`]
//! still behaves correctly given this, matching the original's own
//! "clipboard unavailable" fallback exactly.
//!
//! Because nothing yet WRITES to `Y_REGS`/`last_cmdline`/the
//! `"="`-register expression/the last-inserted-text state, every
//! named/numbered register (`@a`-`@z`, `@"`, `@0`-`@9`, `@-`, `@*`,
//! `@+`) and most special registers correctly, faithfully evaluate to
//! empty/`v:null` today - not a stub, but the genuinely correct
//! behavior for a session in which nothing has ever yanked, deleted,
//! put, or run an Ex command yet.
//!
//! [`get_reg_contents`]'s `kGRegList` flag (return a `List`, one item
//! per register line, rather than a joined string) is now real too,
//! used by both the `getreg()`/`getreginfo()` builtins
//! (`eval/funcs.rs`). [`get_register_name`]/[`get_unname_register`]
//! (the small `register.h` inverse of [`op_reg_index`], and the index
//! of the register `""` currently points to) are also translated -
//! `get_unname_register` always returns `-1` today (`Y_PREVIOUS` is
//! always `None`, see its own doc comment), matching a real session
//! in which nothing has ever performed a genuine yank.
//!
//! `@.` (last inserted text) reads the real `last_insert` file-static
//! via [`crate::insert::get_last_insert_save`], which owns it (matching
//! `insert.c`'s own placement). It reports nothing until something
//! calls `set_last_insert`, which only real insert-mode text entry
//! does - correct for every session that has not yet inserted text.
//!
//! Deferred:
//! - `@Ctrl-F`/`@Ctrl-P`/`@Ctrl-W`/`@Ctrl-A`/`@Ctrl-L` ("under cursor"
//!   pseudo-registers): the original's own `get_spec_reg` immediately
//!   returns `false` for these whenever `errmsg` is `false` - which
//!   [`get_reg_contents`]'s own call always passes - so this crate's
//!   `get_spec_reg` translation matches that exactly and never needs
//!   `file_name_at_cursor`/`find_ident_under_cursor`/`ml_get_buf`
//!   (`errmsg = true` is unreachable from any real caller in this
//!   crate today).
//! - Everything else: `yank_register_mline`, `get_default_register_name`
//!   (`'clipboard'`-driven default-register selection), `op_reg_iter`
//!   (ShaDa register enumeration), and the entire real
//!   yank/delete/put/ShaDa-restore write side (`do_put`, `op_yank`,
//!   `op_delete`, `shada.c`'s register entries) - no real caller
//!   exists for any of these yet.

use crate::register_defs::{greg_flags, RegContents, YankregT, YregModeT, NUM_REGISTERS, PLUS_REGISTER, STAR_REGISTER};

/// Whether register `regname` is inserted literally
/// (`is_literal_register`, `register.h`).
#[must_use]
pub fn is_literal_register(regname: i32) -> bool {
    regname == i32::from(b'*')
        || regname == i32::from(b'+')
        || crate::macros_defs::ascii_isalnum(regname)
}

/// Whether `regname` appends rather than replaces
/// (`is_append_register`, `register.h`).
#[must_use]
pub fn is_append_register(regname: i32) -> bool {
    crate::macros_defs::ascii_isupper(regname)
}

/// Return an owned deep copy of register `name` (`copy_register`).
///
/// # Safety
/// Touches shared register-selection state through
/// [`get_yank_register`].
#[must_use]
pub unsafe fn copy_register(name: i32) -> YankregT {
    let register = unsafe { get_yank_register(name, YregModeT::Paste) };
    let mut copy = unsafe { &*register }.clone();
    if copy.y_array.as_ref().is_some_and(Vec::is_empty) {
        copy.y_array = None;
    }
    copy
}

/// Store one line in register `regname`, appending for uppercase names
/// (`stuff_yank`).
///
/// # Safety
/// Mutates shared register storage and reads the system clock.
unsafe fn stuff_yank(regname: i32, text: &[u8]) -> i32 {
    if regname != 0 && !valid_yank_reg(regname, true) {
        return crate::vim_defs::FAIL;
    }
    if regname == i32::from(b'_') {
        return crate::vim_defs::OK;
    }
    let end = text
        .iter()
        .position(|&byte| byte == crate::ascii_defs::NUL)
        .unwrap_or(text.len());
    let text = &text[..end];
    let register = unsafe { get_yank_register(regname, YregModeT::Yank) };
    let register = unsafe { &mut *register };
    if is_append_register(regname)
        && let Some(lines) = register.y_array.as_mut()
        && let Some(last) = lines.last_mut()
    {
        last.extend_from_slice(text);
    } else {
        free_register(register);
        register.y_array = Some(vec![text.to_vec()]);
        register.y_type = crate::normal_defs::MotionType::CharWise;
    }
    register.timestamp = crate::os::time::os_time();
    crate::vim_defs::OK
}

/// Register selected when macro recording began (`do_record`'s
/// function-local static `regname`).
static RECORD_REGNAME: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);

/// Start or stop macro recording (`do_record`).
///
/// Display updates are omitted. A genuinely registered RecordingEnter
/// or RecordingLeave handler still needs the full `v:event` payload
/// lifecycle and stops at that point.
///
/// # Safety
/// Mutates shared recording, register and autocmd state.
pub unsafe fn do_record(c: i32) -> i32 {
    let globals = crate::globals::GLOBALS.as_ptr();
    if unsafe { (*globals).reg_recording } == 0 {
        if c < 0
            || (!crate::macros_defs::ascii_isalnum(c)
                && c != i32::from(b'"'))
        {
            return crate::vim_defs::FAIL;
        }
        unsafe {
            (*globals).reg_recording = c;
            *RECORD_REGNAME.get_mut() = c;
        }
        if crate::autocmd::has_event(
            crate::autocmd_defs::EventT::RecordingEnter,
        ) {
            unimplemented!(
                "do_record: RecordingEnter handlers need the v:event lifecycle"
            );
        }
        return crate::vim_defs::OK;
    }

    if crate::autocmd::has_event(
        crate::autocmd_defs::EventT::RecordingLeave,
    ) {
        unimplemented!(
            "do_record: RecordingLeave handlers need the v:event lifecycle"
        );
    }
    let mut recorded = unsafe { crate::input::get_recorded() };
    let len = crate::keycodes::vim_unescape_ks(&mut recorded);
    recorded.truncate(len);
    let recording = unsafe { (*globals).reg_recording };
    unsafe {
        (*globals).reg_recorded = recording;
        (*globals).reg_recording = 0;
    }

    let previous = unsafe { *Y_PREVIOUS.get_mut() };
    let result =
        unsafe { stuff_yank(*RECORD_REGNAME.get_mut(), &recorded) };
    unsafe { *Y_PREVIOUS.get_mut() = previous };
    result
}

/// Queue a pending Insert-mode restart after any register text
/// (`put_reedit_in_typebuf`).
///
/// # Safety
/// Touches `GLOBALS.restart_edit` and the shared typeahead buffer.
unsafe fn put_reedit_in_typebuf(silent: bool) {
    let restart = unsafe { crate::globals::GLOBALS.get_mut() }.restart_edit;
    if restart == i32::from(crate::ascii_defs::NUL) {
        return;
    }

    let mut command = [
        crate::keycodes_defs::K_SPECIAL,
        crate::keycodes_defs::KS_EXTRA,
        crate::keycodes_defs::KE_COMMAND,
        b's',
        b't',
        b'a',
        b'r',
        b't',
        b'i',
        crate::ascii_defs::CAR,
    ];
    command[8] = match restart {
        value if value == i32::from(b'R') => b'r',
        value if value == i32::from(b'V') => b'g',
        value if value == i32::from(b'A') => b'!',
        _ => b'i',
    };
    if crate::input::ins_typebuf(
        &command,
        crate::input_defs::RemapValues::None as i32,
        0,
        true,
        silent,
    ) == crate::vim_defs::OK
    {
        unsafe { crate::globals::GLOBALS.get_mut() }.restart_edit = 0;
    }
}

/// Queue register text for execution (`put_in_typebuf`).
///
/// # Safety
/// Forwarded from [`put_reedit_in_typebuf`] and
/// [`crate::input::ins_typebuf`].
unsafe fn put_in_typebuf(
    text: &[u8],
    escape: bool,
    colon: bool,
    silent: bool,
) -> i32 {
    unsafe { put_reedit_in_typebuf(silent) };
    if colon
        && crate::input::ins_typebuf(
            b"\n",
            crate::input_defs::RemapValues::None as i32,
            0,
            true,
            silent,
        ) != crate::vim_defs::OK
    {
        return crate::vim_defs::FAIL;
    }

    let escaped;
    let text = if escape {
        escaped = crate::keycodes::vim_strsave_escape_ks(text);
        escaped.as_slice()
    } else {
        text
    };
    let remap = if escape {
        crate::input_defs::RemapValues::None as i32
    } else {
        crate::input_defs::RemapValues::Yes as i32
    };
    if crate::input::ins_typebuf(text, remap, 0, true, silent)
        != crate::vim_defs::OK
    {
        return crate::vim_defs::FAIL;
    }
    if colon {
        crate::input::ins_typebuf(
            b":",
            crate::input_defs::RemapValues::None as i32,
            0,
            true,
            silent,
        )
    } else {
        crate::vim_defs::OK
    }
}

/// Join backward-processed Ex-register continuation lines
/// (`execreg_line_continuation`).
///
/// `idx` is updated to the first line consumed. Comment-continuation
/// lines (`"\\ `) participate in the backward search but contribute
/// no output, matching the original.
fn execreg_line_continuation(
    lines: &[Vec<u8>],
    idx: &mut usize,
) -> Vec<u8> {
    let mut command_start = *idx;
    debug_assert!(command_start > 0);
    let command_end = command_start;

    loop {
        command_start -= 1;
        if command_start == 0 {
            break;
        }
        let line = &lines[command_start]
            [crate::charset::skipwhite(&lines[command_start])..];
        if line.first() != Some(&b'\\')
            && !(line.starts_with(b"\"\\ "))
        {
            break;
        }
    }

    let mut output = lines[command_start].clone();
    for line in &lines[command_start + 1..=command_end] {
        let line = &line[crate::charset::skipwhite(line)..];
        if line.first() == Some(&b'\\') {
            output.extend_from_slice(&line[1..]);
        }
    }
    *idx = command_start;
    output
}

/// Last register executed by [`do_execreg`] (`execreg_lastc`).
static EXECR_LASTC: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);

/// Queue a register for execution (`do_execreg`).
///
/// Message display is omitted. The expression-register branch reaches
/// the existing `eval_to_string(..., use_simple_function=true)` gap
/// only when an expression was actually stored.
///
/// # Safety
/// Mutates shared register, typeahead and editor global state.
pub unsafe fn do_execreg(
    mut regname: i32,
    colon: bool,
    addcr: bool,
    silent: bool,
) -> i32 {
    if regname == i32::from(b'@') {
        regname = unsafe { *EXECR_LASTC.get_mut() };
        if regname == 0 {
            return crate::vim_defs::FAIL;
        }
    }
    if regname == i32::from(b'%')
        || regname == i32::from(b'#')
        || !valid_yank_reg(regname, false)
    {
        return crate::vim_defs::FAIL;
    }
    unsafe { *EXECR_LASTC.get_mut() = regname };

    if regname == i32::from(b'_') {
        return crate::vim_defs::OK;
    }
    if regname == i32::from(b':') {
        let globals = crate::globals::GLOBALS.as_ptr();
        let Some(command) = (unsafe { (*globals).last_cmdline.clone() }) else {
            return crate::vim_defs::FAIL;
        };
        unsafe { (*globals).new_last_cmdline = None };
        let controls: Vec<u8> = (1..=31).collect();
        let escaped = unsafe {
            crate::strings::vim_strsave_escaped_ext(
                &command,
                &controls,
                crate::ascii_defs::CTRL_V,
                false,
            )
        };
        let text = if unsafe { (*globals).Visual.active }
            && escaped.starts_with(b"'<,'>")
        {
            &escaped[5..]
        } else {
            escaped.as_slice()
        };
        return unsafe { put_in_typebuf(text, true, true, silent) };
    }
    if regname == i32::from(b'=') {
        let Some(expression) = (unsafe { get_expr_line() }) else {
            return crate::vim_defs::FAIL;
        };
        return unsafe { put_in_typebuf(&expression, true, colon, silent) };
    }
    if regname == i32::from(b'.') {
        let Some(inserted) = get_last_insert_save() else {
            return crate::vim_defs::FAIL;
        };
        return unsafe { put_in_typebuf(&inserted, false, colon, silent) };
    }

    let register = unsafe { get_yank_register(regname, YregModeT::Paste) };
    let (Some(lines), yank_type) =
        (unsafe { ((*register).y_array.clone(), (*register).y_type) })
    else {
        return crate::vim_defs::FAIL;
    };
    let remap = if colon {
        crate::input_defs::RemapValues::None as i32
    } else {
        crate::input_defs::RemapValues::Yes as i32
    };
    unsafe { put_reedit_in_typebuf(silent) };
    let line_count = lines.len();
    let mut index = line_count;
    while index > 0 {
        index -= 1;
        let add_newline =
            yank_type == crate::normal_defs::MotionType::LineWise
            || index < line_count - 1
            || addcr;
        if add_newline
            && crate::input::ins_typebuf(
                b"\n",
                remap,
                0,
                true,
                silent,
            ) != crate::vim_defs::OK
        {
            return crate::vim_defs::FAIL;
        }

        let text = if colon
            && index > 0
            && {
                let line =
                    &lines[index][crate::charset::skipwhite(&lines[index])..];
                line.first() == Some(&b'\\') || line.starts_with(b"\"\\ ")
            }
        {
            execreg_line_continuation(&lines, &mut index)
        } else {
            lines[index].clone()
        };
        let escaped = crate::keycodes::vim_strsave_escape_ks(&text);
        if crate::input::ins_typebuf(
            &escaped,
            remap,
            0,
            true,
            silent,
        ) != crate::vim_defs::OK
        {
            return crate::vim_defs::FAIL;
        }
        if colon
            && crate::input::ins_typebuf(
                b":",
                remap,
                0,
                true,
                silent,
            ) != crate::vim_defs::OK
        {
            return crate::vim_defs::FAIL;
        }
    }
    let globals = crate::globals::GLOBALS.as_ptr();
    unsafe {
        (*globals).reg_executing =
            if regname == 0 { i32::from(b'"') } else { regname };
        (*globals).pending_end_reg_executing = false;
    }
    crate::vim_defs::OK
}

/// Insert register contents into the stuff buffer (`insert_reg`).
///
/// The small-delete register's characterwise editing branch still
/// needs `do_put`/real buffer mutation and stops exactly there. Every
/// other named, numbered and special-register path is complete.
///
/// # Safety
/// `reg`, when present, must remain live for the call. Global buffer,
/// register, interrupt and last-insert state must be valid.
pub unsafe fn insert_reg(
    regname: i32,
    reg: Option<&YankregT>,
    literally_arg: bool,
) -> i32 {
    let literally = literally_arg || is_literal_register(regname);
    if unsafe { crate::globals::GLOBALS.get_mut() }.got_int {
        return crate::vim_defs::FAIL;
    }
    if regname != i32::from(crate::ascii_defs::NUL)
        && !valid_yank_reg(regname, false)
    {
        return crate::vim_defs::FAIL;
    }

    if regname == i32::from(b'.') {
        return unsafe {
            crate::insert::stuff_inserted(
                i32::from(crate::ascii_defs::NUL),
                1,
                true,
            )
        };
    }
    if let Some((value, _allocated)) =
        unsafe { get_spec_reg(regname, true) }
    {
        let Some(value) = value else {
            return crate::vim_defs::FAIL;
        };
        crate::input::stuffescaped(&value, literally);
        return crate::vim_defs::OK;
    }

    let owned;
    let register = if let Some(reg) = reg {
        reg
    } else {
        let pointer =
            unsafe { get_yank_register(regname, YregModeT::Paste) };
        owned = unsafe { &*pointer }.clone();
        &owned
    };
    let Some(lines) = register.y_array.as_ref() else {
        return crate::vim_defs::FAIL;
    };
    for (index, line) in lines.iter().enumerate() {
        if regname == i32::from(b'-')
            && register.y_type == crate::normal_defs::MotionType::CharWise
        {
            unimplemented!(
                "insert_reg: characterwise small-delete insertion needs do_put"
            );
        }

        crate::input::stuffescaped(line, literally);
        if register.y_type == crate::normal_defs::MotionType::LineWise
            || index + 1 < lines.len()
        {
            crate::input::stuffchar_readbuff(i32::from(b'\n'));
        }
    }
    crate::vim_defs::OK
}

/// Convert a register name character to its `Y_REGS` index
/// (`op_reg_index`). Returns `None` for a name with no direct slot
/// (matching the original's own `-1`) - digits map to `0..=9`,
/// letters (either case) to `10..=35`, `'-'`/`'*'`/`'+'` to their own
/// named constants.
#[must_use]
pub fn op_reg_index(regname: i32) -> Option<usize> {
    if (i32::from(b'0')..=i32::from(b'9')).contains(&regname) {
        Some((regname - i32::from(b'0')) as usize)
    } else if (i32::from(b'a')..=i32::from(b'z')).contains(&regname) {
        Some((regname - i32::from(b'a')) as usize + 10)
    } else if (i32::from(b'A')..=i32::from(b'Z')).contains(&regname) {
        Some((regname - i32::from(b'A')) as usize + 10)
    } else if regname == i32::from(b'-') {
        Some(crate::register_defs::DELETION_REGISTER)
    } else if regname == i32::from(b'*') {
        Some(STAR_REGISTER)
    } else if regname == i32::from(b'+') {
        Some(PLUS_REGISTER)
    } else {
        None
    }
}

/// The character name of the register with the given `Y_REGS` index
/// (`get_register_name`), the inverse of [`op_reg_index`] -
/// `-1` maps to `'"'` (the unnamed register), matching the original's
/// own special case for [`get_unname_register`]'s `-1` "no previous
/// register" sentinel.
#[must_use]
pub fn get_register_name(num: i32) -> i32 {
    if num == -1 {
        i32::from(b'"')
    } else if num < 10 {
        num + i32::from(b'0')
    } else if num == crate::register_defs::DELETION_REGISTER as i32 {
        i32::from(b'-')
    } else if num == STAR_REGISTER as i32 {
        i32::from(b'*')
    } else if num == PLUS_REGISTER as i32 {
        i32::from(b'+')
    } else {
        num + i32::from(b'a') - 10
    }
}

/// The index of the register `""` (unnamed) currently points to
/// (`get_unname_register`) - always `-1` today, matching `Y_PREVIOUS`
/// always being `None` (nothing performs a real yank yet).
///
/// # Safety
/// Touches `Y_PREVIOUS` (a `GlobalCell`) - no overlapping live access.
#[must_use]
pub unsafe fn get_unname_register() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    match unsafe { *Y_PREVIOUS.get_mut() } {
        Some(idx) => idx as i32,
        None => -1,
    }
}

/// Check if `regname` is a valid name of a yank register
/// (`valid_yank_reg`).
///
/// There is no check for `0` (the default/unnamed register) - the
/// caller must do this, matching the original's own documented
/// requirement. The black hole register `'_'` is regarded as valid.
#[must_use]
pub fn valid_yank_reg(regname: i32, writing: bool) -> bool {
    (regname > 0 && u8::try_from(regname).is_ok_and(|b| b.is_ascii_alphanumeric()))
        || (!writing && regname > 0 && u8::try_from(regname).is_ok_and(|b| b"/#.%:=".contains(&b)))
        || regname == i32::from(b'"')
        || regname == i32::from(b'-')
        || regname == i32::from(b'_')
        || regname == i32::from(b'*')
        || regname == i32::from(b'+')
}

/// Try to read from or write to the system clipboard (`get_clipboard`,
/// via `adjust_clipboard_name`/the `'clipboard'`-option-driven
/// provider dispatch).
///
/// Always returns `false` (no provider registered) - this crate has
/// no clipboard-provider integration translated yet (needs a real Lua/
/// external-UI provider dispatch, `ui_client.c`/`provider#clipboard#`,
/// a separate, substantial undertaking). Every real caller in this
/// crate already handles a `false` return exactly as the original
/// does for "no provider available", so this is a faithful, not a
/// merely convenient, default for today's reality.
#[must_use]
fn get_clipboard(_regname: i32) -> bool {
    false
}

/// The 39 yank/delete/numbered/named registers (`y_regs[]`).
static Y_REGS: std::sync::LazyLock<crate::globals::GlobalCell<[YankregT; NUM_REGISTERS]>> =
    std::sync::LazyLock::new(|| crate::globals::GlobalCell::new(std::array::from_fn(|_| YankregT::default())));

/// Index into `Y_REGS` of the last-written register, for
/// unnamed-paste fallback (`y_previous`, a `yankreg_T *` in the
/// original - modeled as an index rather than a pointer, since
/// `Y_REGS` is a fixed-size array rather than individually heap-
/// allocated registers). `None` matches the original's own initial
/// `NULL` - stays `None` forever today, since nothing ever performs a
/// real yank (`YREG_YANK` mode) yet.
static Y_PREVIOUS: crate::globals::GlobalCell<Option<usize>> = crate::globals::GlobalCell::new(None);

/// A permanently-empty register, returned by [`get_yank_register`] for
/// `'*'`/`'+'` in `YregModeT::Put` mode when the clipboard is
/// unavailable (`static yankreg_T empty_reg` in the original - a
/// function-local `static` there, promoted to file scope here since
/// Rust has no function-local `static` initializer needing per-call
/// re-initialization semantics the original doesn't rely on either).
static EMPTY_REG: crate::globals::GlobalCell<YankregT> = crate::globals::GlobalCell::new(YankregT {
    y_array: None,
    y_type: crate::normal_defs::MotionType::CharWise,
    y_width: 0,
    timestamp: 0,
});

/// The register at `Y_REGS` index `reg` (`get_y_register`).
///
/// The original indexes `y_regs[]` unchecked; this bounds-checks and
/// returns `None` past the end instead.
///
/// # Safety
/// Touches `Y_REGS` (a `GlobalCell`) - no overlapping live access.
#[must_use]
pub unsafe fn get_y_register(reg: usize) -> Option<*mut YankregT> {
    // SAFETY: forwarded from this function's own safety doc.
    let regs = unsafe { Y_REGS.get_mut() };
    if reg >= regs.len() {
        return None;
    }
    Some(&mut regs[reg])
}

/// Returns the register named by `name` (`op_reg_get`), or `None`
/// when the name has no direct register slot.
///
/// # Safety
/// Touches the `Y_REGS` file-static; the returned pointer must not
/// overlap another mutable access to that storage.
#[must_use]
pub unsafe fn op_reg_get(name: i32) -> Option<*const YankregT> {
    let idx = op_reg_index(name)?;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { get_y_register(idx) }.map(|ptr| ptr.cast_const())
}

/// Replaces the register named by `name` with `reg` (`op_reg_set`).
///
/// Returns `false` when the name has no direct slot. When
/// `is_unnamed` is true, unnamed-register fallback is redirected to
/// the replaced slot.
///
/// # Safety
/// Mutates the `Y_REGS` and possibly `Y_PREVIOUS` file-statics.
pub unsafe fn op_reg_set(name: i32, reg: YankregT, is_unnamed: bool) -> bool {
    let Some(idx) = op_reg_index(name) else {
        return false;
    };
    // Assigning the owned value drops the previous register contents,
    // exactly replacing the original's free-then-shallow-copy pair.
    unsafe { Y_REGS.get_mut()[idx] = reg };
    if is_unnamed {
        unsafe { *Y_PREVIOUS.get_mut() = Some(idx) };
    }
    true
}

/// Selects the register named by `name` as the previous yank register
/// (`op_reg_set_previous`).
///
/// Returns `false` and leaves the previous selection untouched when
/// the name has no direct register slot.
///
/// # Safety
/// Mutates the `Y_PREVIOUS` file-static.
pub unsafe fn op_reg_set_previous(name: i32) -> bool {
    let Some(idx) = op_reg_index(name) else {
        return false;
    };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *Y_PREVIOUS.get_mut() = Some(idx) };
    true
}

/// Releases the owned contents of yank register `reg`
/// (`free_register`).
///
/// `YankregT`'s `Option<Vec<Vec<u8>>>` owns both the line array and
/// every line, so the original's nested frees collapse into assigning
/// `None`. Metadata is deliberately retained, as in the original.
pub fn free_register(reg: &mut YankregT) {
    reg.y_array = None;
}

/// Updates a blockwise register's width from its contents
/// (`update_yankreg_width`).
///
/// The stored width is the maximum screen-cell width minus one, and
/// never shrinks. Non-blockwise registers are untouched.
///
/// # Safety
/// Reads multibyte option state through
/// [`crate::mbyte::mb_string2cells`].
pub unsafe fn update_yankreg_width(reg: &mut YankregT) {
    if reg.y_type != crate::normal_defs::MotionType::BlockWise {
        return;
    }

    let maxlen = reg
        .y_array
        .as_deref()
        .unwrap_or(&[])
        .iter()
        // SAFETY: forwarded from this function's own safety doc.
        .map(|line| unsafe { crate::mbyte::mb_string2cells(line) })
        .max()
        .unwrap_or(0);
    let measured = i32::try_from(maxlen).expect("register line width must fit in i32") - 1;
    reg.y_width = reg.y_width.max(measured);
}

/// Shifts numbered delete registers `"1` through `"9`
/// (`shift_delete_registers`).
///
/// Register `"9` is discarded, `"8` becomes `"9`, and so on. The
/// new `"1` is empty. The C implementation shallow-copies each owned
/// pointer then nulls only `"1`'s array; moving each Rust value avoids
/// duplicate ownership while explicitly preserving `"1`'s metadata.
///
/// When `y_append` is false, the unnamed register is redirected to
/// the newly emptied `"1`.
///
/// # Safety
/// Mutates the `Y_REGS` and `Y_PREVIOUS` file-statics.
pub unsafe fn shift_delete_registers(y_append: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let regs = unsafe { Y_REGS.get_mut() };
    let retained_type = regs[1].y_type;
    let retained_width = regs[1].y_width;
    let retained_timestamp = regs[1].timestamp;

    free_register(&mut regs[9]);
    for n in (2..=9).rev() {
        regs[n] = std::mem::take(&mut regs[n - 1]);
    }
    regs[1] = YankregT {
        y_array: None,
        y_type: retained_type,
        y_width: retained_width,
        timestamp: retained_timestamp,
    };

    if !y_append {
        unsafe { *Y_PREVIOUS.get_mut() = Some(1) };
    }
}

/// Releases the contents of every register (`clear_registers`).
///
/// This is compiled only for exit-time cleanup in the original. It is
/// always available here, where dropping each owned line array is
/// safe and useful in tests as well.
///
/// # Safety
/// Mutates the `Y_REGS` file-static.
pub unsafe fn clear_registers() {
    // SAFETY: forwarded from this function's own safety doc.
    for reg in unsafe { Y_REGS.get_mut() } {
        free_register(reg);
    }
}

/// Whether the register `regname` holds linewise content, also
/// handing back the register itself (`yank_register_mline`).
///
/// An invalid register name, or the black hole `"_` (which is always
/// empty), answers `false` with no register.
///
/// The original writes the register through a `yankreg_T **reg`
/// out-parameter and returns the linewise flag separately; both ride
/// in the returned tuple here.
///
/// # Safety
/// Forwarded from [`get_yank_register`]'s own safety doc.
#[must_use]
pub unsafe fn yank_register_mline(regname: i32) -> (bool, Option<*mut YankregT>) {
    if regname != 0 && !valid_yank_reg(regname, false) {
        return (false, None);
    }
    if regname == i32::from(b'_') {
        // black hole is always empty
        return (false, None);
    }
    // SAFETY: forwarded from this function's own safety doc.
    let reg = unsafe { get_yank_register(regname, YregModeT::Paste) };
    // SAFETY: get_yank_register always returns a live pointer.
    let is_line = unsafe { (*reg).y_type } == crate::normal_defs::MotionType::LineWise;
    (is_line, Some(reg))
}

/// Get the yank register for `regname` (`get_yank_register`).
///
/// # Safety
/// Touches `Y_REGS`/`Y_PREVIOUS`/`EMPTY_REG` (`GlobalCell`s) - no
/// overlapping live access.
#[must_use]
pub unsafe fn get_yank_register(regname: i32, mode: YregModeT) -> *mut YankregT {
    if (mode == YregModeT::Paste || mode == YregModeT::Put) && get_clipboard(regname) {
        // Unreachable today - get_clipboard always returns false (see
        // its own doc comment) - kept for structural fidelity in case
        // a future clipboard-provider integration makes this real.
        unreachable!("get_clipboard never succeeds yet");
    } else if mode == YregModeT::Put && (regname == i32::from(b'*') || regname == i32::from(b'+')) {
        // In case the clipboard isn't available and we aren't actually
        // pasting, return an empty register.
        return unsafe { EMPTY_REG.get_mut() };
    } else if mode != YregModeT::Yank
        && (regname == 0 || regname == i32::from(b'"') || regname == i32::from(b'*') || regname == i32::from(b'+'))
    {
        // In case the clipboard isn't available, paste from the
        // previously used register.
        if let Some(idx) = unsafe { *Y_PREVIOUS.get_mut() } {
            // SAFETY: forwarded from this function's own safety doc.
            return unsafe { &mut Y_REGS.get_mut()[idx] };
        }
    }

    // When not 0-9, a-z, A-Z, or '-'/'*'/'+': use register 0.
    let i = op_reg_index(regname).unwrap_or(0);
    // SAFETY: forwarded from this function's own safety doc.
    let reg: *mut YankregT = unsafe { &mut Y_REGS.get_mut()[i] };

    if mode == YregModeT::Yank {
        // Remember the written register for unnamed paste.
        unsafe { *Y_PREVIOUS.get_mut() = Some(i) };
    }
    reg
}

/// Returns the previously-used yank register (`get_y_previous`), or a
/// null pointer if none has been used yet (`Y_PREVIOUS` is `None`) -
/// always null today, since nothing yet performs a real yank
/// (`YREG_YANK` mode); see `Y_PREVIOUS`'s own doc comment.
///
/// # Safety
/// Touches `Y_REGS`/`Y_PREVIOUS` (`GlobalCell`s) - no overlapping live
/// access.
#[must_use]
pub unsafe fn get_y_previous() -> *mut YankregT {
    match unsafe { *Y_PREVIOUS.get_mut() } {
        // SAFETY: forwarded from this function's own safety doc.
        Some(idx) => unsafe { &mut Y_REGS.get_mut()[idx] },
        None => std::ptr::null_mut(),
    }
}

/// The expression evaluated for the `"="` register (`expr_line`, a
/// file-static in the original). Always `None` today: nothing in this
/// crate translates `:let @= = ...`/`c_CTRL-R_=`-style assignment yet,
/// so the `"="` register always evaluates to `v:null` - a genuinely
/// correct, not merely stubbed, state for a session where nothing has
/// ever set it.
static EXPR_LINE: crate::globals::GlobalCell<Option<Vec<u8>>> = crate::globals::GlobalCell::new(None);

/// Set the expression for the `"="` register (`set_expr_line`).
///
/// Not yet called by anything real in this crate (needs the
/// `c_CTRL-R_=`/`:let @=`-style assignment machinery), but translated
/// for completeness alongside its own `EXPR_LINE` storage.
pub fn set_expr_line(new_line: Option<Vec<u8>>) {
    unsafe { *EXPR_LINE.get_mut() = new_line };
}

/// Get the result of the `"="` register expression (`get_expr_line`).
///
/// # Safety
/// Forwarded from `crate::eval::eval::eval_to_string`'s own safety
/// doc (only reached when `EXPR_LINE` is actually `Some`, which
/// nothing in this crate constructs yet - see this module's own doc
/// comment).
#[must_use]
pub unsafe fn get_expr_line() -> Option<Vec<u8>> {
    let expr_line = unsafe { EXPR_LINE.get_mut() }.clone()?;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::eval::eval_to_string(&expr_line, true, false) }
}

/// Get the `"="` register expression itself, without evaluating it
/// (`get_expr_line_src`).
#[must_use]
pub fn get_expr_line_src() -> Option<Vec<u8>> {
    unsafe { EXPR_LINE.get_mut() }.clone()
}

/// Get the last inserted text, with a trailing `<Esc>` removed
/// (`get_last_insert_save`).
///
/// Delegates to [`crate::insert::get_last_insert_save`], which owns
/// the real `last_insert` file-static (matching `insert.c`'s own
/// placement); this module is only its reader, via `get_spec_reg`.
#[must_use]
fn get_last_insert_save() -> Option<Vec<u8>> {
    // SAFETY: reads insert.rs's own LAST_INSERT/LAST_INSERT_SKIP
    // file-statics, serialized by this crate's test lock.
    unsafe { crate::insert::get_last_insert_save() }
}

/// Get the alternate file name (`@#`) (`getaltfname`).
///
/// `buflist_name_nr(0)` already resolves `fnum == 0` via
/// `GLOBALS.curwin.w_alt_fnum` internally (matching the original's own
/// `buflist_findnr`, whose own first statement is exactly `if (nr ==
/// 0) { nr = curwin->w_alt_fnum; }`) - so this is a direct, complete
/// translation, not the earlier narrower "only w_alt_fnum == 0"
/// shortcut (now unblocked since `buflist_findnr`/`buflist_name_nr`
/// both exist for real).
///
/// `errmsg` (whether to report "no alternate file name" as an error)
/// is accepted for signature fidelity but never actually matters yet:
/// every real caller in this crate passes `false` (see this module's
/// own doc comment), and the `None` path never displays a message
/// regardless (message display, not tractable, matches this crate's
/// established policy elsewhere).
///
/// # Safety
/// Forwarded from [`crate::buffer::buflist_name_nr`]'s own safety doc.
#[must_use]
unsafe fn getaltfname(_errmsg: bool) -> Option<Vec<u8>> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::buffer::buflist_name_nr(0) }.map(|(fname, _lnum)| fname)
}

/// Get the value of a special register, if `regname` names one
/// (`get_spec_reg`).
///
/// Returns `Some((value, allocated))` when `regname` IS a special
/// register (`value` may itself be `None`, e.g. an as-yet-unset `"."`/
/// `":"` register) - `allocated` has no real meaning in this crate's
/// own always-owned `Option<Vec<u8>>` idiom (kept only for structural
/// fidelity with the original's "did this need a fresh allocation or
/// point at existing storage" distinction) - `None` when `regname`
/// isn't a special register at all, matching the original's own
/// `false` return.
///
/// # Safety
/// Forwarded from `getaltfname`'s own safety doc (only reached for
/// `regname == '#'`).
#[must_use]
unsafe fn get_spec_reg(regname: i32, errmsg: bool) -> Option<(Option<Vec<u8>>, bool)> {
    match u8::try_from(regname).ok() {
        Some(b'%') => {
            // file name
            let b_fname = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf }.b_fname.clone();
            Some((b_fname, false))
        }
        Some(b'#') => {
            // alternate file name
            // SAFETY: forwarded from this function's own safety doc.
            Some((unsafe { getaltfname(errmsg) }, false))
        }
        Some(b'=') => {
            // result of expression
            // SAFETY: forwarded from this function's own safety doc.
            Some((unsafe { get_expr_line() }, true))
        }
        Some(b':') => {
            // last command line
            Some((unsafe { crate::globals::GLOBALS.get_mut() }.last_cmdline.clone(), false))
        }
        Some(b'/') => {
            // last search-pattern
            Some((crate::search::last_search_pat(), false))
        }
        Some(b'.') => {
            // last inserted text
            Some((get_last_insert_save(), true))
        }
        // Ctrl_F/Ctrl_P (filename/path under cursor), Ctrl_W/Ctrl_A
        // (word/WORD under cursor), Ctrl_L (line under cursor): the
        // original's own get_spec_reg immediately `return false;` for
        // all five of these whenever `!errmsg` - which is always true
        // for get_reg_contents's own real call - so this crate's own
        // translation matches that exactly, needing none of
        // file_name_at_cursor/find_ident_under_cursor/ml_get_buf.
        Some(0x06 | 0x10 | 0x17 | 0x01 | 0x0c) if !errmsg => None,
        Some(0x06 | 0x10 | 0x17 | 0x01 | 0x0c) => {
            unimplemented!(
                "get_spec_reg: errmsg=true for a cursor-relative pseudo-register is unreachable \
                 from any real caller today, not yet translated"
            );
        }
        Some(b'_') => {
            // black hole: always empty.
            Some((Some(Vec::new()), false))
        }
        _ => None,
    }
}

/// Wrap `s` for `get_reg_contents`'s own return value
/// (`get_reg_wrap_one_line`): a plain string when `flags` doesn't
/// include `greg_flags::LIST`, or a freshly-allocated 1-element `List`
/// containing `s` when it does. Returns `None` when `s` itself is
/// `None` (matching the original's own `retval == NULL` early return,
/// checked by every real caller BEFORE calling the original
/// `get_reg_wrap_one_line` at all - folded into this one helper here
/// since Rust's `?` operator makes that trivial).
fn get_reg_wrap_one_line(s: Option<Vec<u8>>, flags: u32) -> Option<RegContents> {
    let s = s?;
    if flags & greg_flags::LIST == 0 {
        return Some(RegContents::Str(s));
    }
    let list = crate::eval::typval::tv_list_alloc(1);
    // SAFETY: `list` was just allocated above, a valid, exclusively-
    // owned pointer.
    unsafe { crate::eval::typval::tv_list_append_string(list, Some(&s)) };
    Some(RegContents::List(list))
}

/// Gets the contents of a register (`get_reg_contents`).
/// @remark Used for `@r` in expressions and for `getreg()`.
///
/// Returns `None` for an invalid register name or an unset/empty
/// register, matching the original's own `NULL`. Returns
/// [`RegContents::List`] when `flags` includes `greg_flags::LIST`
/// (needed by the `getreg()` builtin, not `@r`'s own real call, which
/// always passes `flags == greg_flags::EXPR_SRC`).
///
/// # Safety
/// Forwarded from `get_spec_reg`'s own safety doc.
#[must_use]
pub unsafe fn get_reg_contents(regname: i32, flags: u32) -> Option<RegContents> {
    // Don't allow using an expression register inside an expression.
    let regname = if regname == i32::from(b'=') {
        if flags & greg_flags::NO_EXPR != 0 {
            return None;
        }
        return if flags & greg_flags::EXPR_SRC != 0 {
            get_reg_wrap_one_line(get_expr_line_src(), flags)
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            get_reg_wrap_one_line(unsafe { get_expr_line() }, flags)
        };
    } else if regname == i32::from(b'@') {
        // "@@" is used for the unnamed register.
        i32::from(b'"')
    } else {
        regname
    };

    // Check for a valid regname.
    if regname != 0 && !valid_yank_reg(regname, false) {
        return None;
    }

    // SAFETY: forwarded from this function's own safety doc.
    if let Some((retval, _allocated)) = unsafe { get_spec_reg(regname, false) } {
        return get_reg_wrap_one_line(retval, flags);
    }

    // SAFETY: forwarded from this function's own safety doc.
    let reg = unsafe { &*get_yank_register(regname, YregModeT::Put) };
    let y_array = reg.y_array.as_ref()?;

    if flags & greg_flags::LIST != 0 {
        let list = crate::eval::typval::tv_list_alloc(y_array.len() as isize);
        for line in y_array {
            // SAFETY: `list` was just allocated above, a valid,
            // exclusively-owned pointer.
            unsafe { crate::eval::typval::tv_list_append_string(list, Some(line)) };
        }
        return Some(RegContents::List(list));
    }

    // Join the lines of the yank register into one string, inserting a
    // newline between lines and after the last line if y_type is
    // LineWise.
    let mut retval = Vec::new();
    for (i, line) in y_array.iter().enumerate() {
        retval.extend_from_slice(line);
        if reg.y_type == crate::normal_defs::MotionType::LineWise || i < y_array.len() - 1 {
            retval.push(b'\n');
        }
    }
    Some(RegContents::Str(retval))
}

/// Get the type of register `regname` (`get_reg_type`).
///
/// Returns `None` for `kMTUnknown` (matching this crate's own
/// established `Option<MotionType>` idiom for the original's `-1`
/// sentinel - see `normal_defs.rs`'s own doc comment for
/// `K_MT_UNKNOWN`). The special registers below (file name, alternate
/// file name, expression, last command line, last search pattern,
/// last inserted text, black hole, and the 4
/// `Ctrl_F`/`Ctrl_P`/`Ctrl_W`/`Ctrl_A` "under cursor" pseudo-registers)
/// are always charwise, matching the original's own `switch`.
///
/// `reg_width` is populated only for a real `BlockWise` register,
/// matching the original's own `reg_width != NULL` guard.
///
/// # Safety
/// Forwarded from [`get_yank_register`]'s own safety doc.
#[must_use]
pub unsafe fn get_reg_type(regname: i32, reg_width: Option<&mut crate::pos_defs::ColnrT>) -> Option<crate::normal_defs::MotionType> {
    match u8::try_from(regname).ok() {
        Some(b'%' | b'#' | b'=' | b':' | b'/' | b'.' | b'_') => {
            return Some(crate::normal_defs::MotionType::CharWise);
        }
        // Ctrl_F / Ctrl_P / Ctrl_W / Ctrl_A ("under cursor" pseudo-registers).
        Some(0x06 | 0x10 | 0x17 | 0x01) => return Some(crate::normal_defs::MotionType::CharWise),
        _ => {}
    }

    if regname != 0 && !valid_yank_reg(regname, false) {
        return None;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let reg = unsafe { &*get_yank_register(regname, YregModeT::Paste) };

    if reg.y_array.is_some() {
        if let Some(w) = reg_width
            && reg.y_type == crate::normal_defs::MotionType::BlockWise
        {
            *w = reg.y_width;
        }
        Some(reg.y_type)
    } else {
        None
    }
}

/// Format a register type as its display string (`format_reg_type`).
///
/// Returns the display form directly as a freshly-owned `Vec<u8>`
/// rather than writing into a caller-provided fixed buffer - matches
/// this crate's established "Rust's own growable buffer needs no
/// pre-sizing dance" simplification (e.g. `winrestcmd`/
/// `vim_strsave_shellescape`).
#[must_use]
pub fn format_reg_type(reg_type: Option<crate::normal_defs::MotionType>, reg_width: crate::pos_defs::ColnrT) -> Vec<u8> {
    match reg_type {
        Some(crate::normal_defs::MotionType::LineWise) => vec![b'V'],
        Some(crate::normal_defs::MotionType::CharWise) => vec![b'v'],
        Some(crate::normal_defs::MotionType::BlockWise) => format!("\x16{}", reg_width + 1).into_bytes(),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TypeaheadGuard(crate::input_defs::TasaveT);
    struct LastInsertResetGuard;
    struct InputRecordingGuard(Option<(crate::input_defs::BuffheaderT, usize)>);

    impl TypeaheadGuard {
        fn save() -> Self {
            let mut saved = crate::input_defs::TasaveT::default();
            crate::input::save_typeahead(&mut saved);
            Self(saved)
        }
    }

    impl Drop for TypeaheadGuard {
        fn drop(&mut self) {
            crate::input::restore_typeahead(&mut self.0);
        }
    }

    impl Drop for LastInsertResetGuard {
        fn drop(&mut self) {
            unsafe { crate::insert::reset_last_insert_for_test() };
        }
    }

    impl InputRecordingGuard {
        fn save() -> Self {
            Self(Some(crate::input::take_recording_state_for_test()))
        }
    }

    impl Drop for InputRecordingGuard {
        fn drop(&mut self) {
            crate::input::restore_recording_state_for_test(
                self.0.take().expect("saved input recording state"),
            );
        }
    }

    fn stuffed_bytes() -> Vec<u8> {
        let mut bytes = Vec::new();
        loop {
            let byte = crate::input::read_readbuffers(true);
            if byte == crate::ascii_defs::NUL {
                return bytes;
            }
            bytes.push(byte);
        }
    }

    struct CmdlineGuard {
        last: Option<Vec<u8>>,
        new_last: Option<Vec<u8>>,
    }

    impl CmdlineGuard {
        fn set(last: Option<Vec<u8>>, new_last: Option<Vec<u8>>) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            Self {
                last: std::mem::replace(&mut globals.last_cmdline, last),
                new_last: std::mem::replace(
                    &mut globals.new_last_cmdline,
                    new_last,
                ),
            }
        }
    }

    impl Drop for CmdlineGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.last_cmdline = self.last.take();
            globals.new_last_cmdline = self.new_last.take();
        }
    }

    #[test]
    fn put_reedit_in_typebuf_queues_the_matching_start_command() {
        let _lock = crate::globals::global_state_test_lock();
        for (restart, suffix) in [(b'I', b'i'), (b'R', b'r'), (b'V', b'g'), (b'A', b'!')] {
            let _typeahead = TypeaheadGuard::save();
            let _restart = unsafe {
                crate::globals::GlobalFieldGuard::install(
                    |globals| &mut globals.restart_edit,
                    i32::from(restart),
                )
            };

            unsafe { put_reedit_in_typebuf(false) };

            assert_eq!(
                crate::input::typebuf_bytes_for_test(),
                vec![
                    crate::keycodes_defs::K_SPECIAL,
                    crate::keycodes_defs::KS_EXTRA,
                    crate::keycodes_defs::KE_COMMAND,
                    b's',
                    b't',
                    b'a',
                    b'r',
                    b't',
                    suffix,
                    crate::ascii_defs::CAR,
                ]
            );
            assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.restart_edit, 0);
        }
    }

    #[test]
    fn is_literal_register_accepts_clipboard_and_alphanumeric_names() {
        for &name in b"*+aZ09" {
            assert!(is_literal_register(i32::from(name)));
        }
    }

    #[test]
    fn is_literal_register_rejects_other_special_names() {
        for &name in b"\"-_:=" {
            assert!(!is_literal_register(i32::from(name)));
        }
        assert!(!is_literal_register(-1));
    }

    #[test]
    fn is_append_register_accepts_only_uppercase_ascii_letters() {
        for &name in b"AZ" {
            assert!(is_append_register(i32::from(name)));
        }
        for &name in b"az09+_" {
            assert!(!is_append_register(i32::from(name)));
        }
        assert!(!is_append_register(-1));
    }

    #[test]
    fn copy_register_deep_copies_lines_and_metadata() {
        let _lock = crate::globals::global_state_test_lock();
        let _registers = RegisterStateGuard::save();
        let _input_recording = InputRecordingGuard::save();
        let index = op_reg_index(i32::from(b'a')).unwrap();
        unsafe {
            Y_REGS.get_mut()[index] = YankregT {
                y_array: Some(vec![b"one".to_vec(), b"two".to_vec()]),
                y_type: crate::normal_defs::MotionType::BlockWise,
                y_width: 7,
                timestamp: 11,
            };
        }

        let mut copy = unsafe { copy_register(i32::from(b'a')) };
        copy.y_array.as_mut().unwrap()[0][0] = b'X';

        assert_eq!(copy.y_type, crate::normal_defs::MotionType::BlockWise);
        assert_eq!(copy.y_width, 7);
        assert_eq!(copy.timestamp, 11);
        assert_eq!(
            unsafe { Y_REGS.get_mut()[index].y_array.as_ref().unwrap()[0].as_slice() },
            b"one"
        );
    }

    #[test]
    fn copy_register_normalizes_an_empty_allocated_array_to_none() {
        let _lock = crate::globals::global_state_test_lock();
        let _registers = RegisterStateGuard::save();
        let _input_recording = InputRecordingGuard::save();
        let index = op_reg_index(i32::from(b'b')).unwrap();
        unsafe {
            Y_REGS.get_mut()[index].y_array = Some(Vec::new());
        }

        assert!(unsafe { copy_register(i32::from(b'b')) }
            .y_array
            .is_none());
    }

    #[test]
    fn stuff_yank_replaces_a_lowercase_register() {
        let _lock = crate::globals::global_state_test_lock();
        let _registers = RegisterStateGuard::save();
        let index = op_reg_index(i32::from(b'a')).unwrap();
        unsafe {
            Y_REGS.get_mut()[index] = YankregT {
                y_array: Some(vec![b"old".to_vec()]),
                y_type: crate::normal_defs::MotionType::LineWise,
                y_width: 7,
                ..Default::default()
            };
        }

        assert_eq!(
            unsafe { stuff_yank(i32::from(b'a'), b"new\0ignored") },
            crate::vim_defs::OK
        );

        let register = &unsafe { Y_REGS.get_mut() }[index];
        assert_eq!(register.y_array.as_deref(), Some([b"new".to_vec()].as_slice()));
        assert_eq!(register.y_type, crate::normal_defs::MotionType::CharWise);
        assert_eq!(register.y_width, 7);
        assert!(register.timestamp > 0);
    }

    #[test]
    fn stuff_yank_appends_to_the_last_line_for_uppercase_names() {
        let _lock = crate::globals::global_state_test_lock();
        let _registers = RegisterStateGuard::save();
        let index = op_reg_index(i32::from(b'b')).unwrap();
        unsafe {
            Y_REGS.get_mut()[index] = YankregT {
                y_array: Some(vec![b"first".to_vec(), b"last".to_vec()]),
                y_type: crate::normal_defs::MotionType::LineWise,
                ..Default::default()
            };
        }

        assert_eq!(
            unsafe { stuff_yank(i32::from(b'B'), b"+more") },
            crate::vim_defs::OK
        );

        let register = &unsafe { Y_REGS.get_mut() }[index];
        assert_eq!(
            register.y_array.as_deref(),
            Some([b"first".to_vec(), b"last+more".to_vec()].as_slice())
        );
        assert_eq!(register.y_type, crate::normal_defs::MotionType::LineWise);
    }

    #[test]
    fn stuff_yank_accepts_black_hole_and_rejects_readonly_registers() {
        let _lock = crate::globals::global_state_test_lock();
        let _registers = RegisterStateGuard::save();
        assert_eq!(
            unsafe { stuff_yank(i32::from(b'_'), b"discard") },
            crate::vim_defs::OK
        );
        assert_eq!(
            unsafe { stuff_yank(i32::from(b'%'), b"invalid") },
            crate::vim_defs::FAIL
        );
    }

    #[test]
    fn do_record_rejects_invalid_names_and_starts_valid_recording() {
        let _lock = crate::globals::global_state_test_lock();
        let _registers = RegisterStateGuard::save();
        let _input_recording = InputRecordingGuard::save();
        let _recording = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.reg_recording,
                0,
            )
        };

        assert_eq!(unsafe { do_record(i32::from(b'!')) }, crate::vim_defs::FAIL);
        assert_eq!(unsafe { do_record(i32::from(b'a')) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.reg_recording,
            i32::from(b'a')
        );
    }

    #[test]
    fn do_record_stops_and_stores_the_recorded_register() {
        let _lock = crate::globals::global_state_test_lock();
        let _registers = RegisterStateGuard::save();
        let _input_recording = InputRecordingGuard::save();
        let _recording = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.reg_recording,
                0,
            )
        };
        let _recorded = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.reg_recorded,
                0,
            )
        };
        unsafe { *Y_PREVIOUS.get_mut() = Some(0) };

        assert_eq!(unsafe { do_record(i32::from(b'b')) }, crate::vim_defs::OK);
        crate::input::set_recorded_state_for_test(
            &[
                b'a',
                crate::keycodes_defs::K_SPECIAL,
                crate::keycodes_defs::KS_SPECIAL,
                crate::keycodes_defs::KE_FILLER,
                b'q',
            ],
            1,
        );
        assert_eq!(unsafe { do_record(0) }, crate::vim_defs::OK);

        let index = op_reg_index(i32::from(b'b')).unwrap();
        assert_eq!(
            unsafe { Y_REGS.get_mut()[index].y_array.as_deref() },
            Some([vec![b'a', crate::keycodes_defs::K_SPECIAL]].as_slice())
        );
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.reg_recorded,
            i32::from(b'b')
        );
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.reg_recording, 0);
        assert_eq!(unsafe { *Y_PREVIOUS.get_mut() }, Some(0));
    }

    #[test]
    fn put_in_typebuf_wraps_colon_commands_in_execution_order() {
        let _lock = crate::globals::global_state_test_lock();
        let _typeahead = TypeaheadGuard::save();
        let _restart = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.restart_edit,
                0,
            )
        };

        assert_eq!(
            unsafe { put_in_typebuf(b"echo 1", true, true, false) },
            crate::vim_defs::OK
        );

        assert_eq!(
            crate::input::typebuf_bytes_for_test(),
            b":echo 1\n"
        );
        assert!(crate::input::typebuf_remap_for_test()
            .iter()
            .all(|&flag| flag == crate::input::RM_NONE as u8));
    }

    #[test]
    fn put_in_typebuf_places_pending_reedit_after_register_text() {
        let _lock = crate::globals::global_state_test_lock();
        let _typeahead = TypeaheadGuard::save();
        let _restart = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.restart_edit,
                i32::from(b'R'),
            )
        };
        let _silent = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.cmd_silent,
                false,
            )
        };

        assert_eq!(
            unsafe { put_in_typebuf(b"text", false, false, true) },
            crate::vim_defs::OK
        );

        let mut expected = b"text".to_vec();
        expected.extend_from_slice(&[
            crate::keycodes_defs::K_SPECIAL,
            crate::keycodes_defs::KS_EXTRA,
            crate::keycodes_defs::KE_COMMAND,
            b's',
            b't',
            b'a',
            b'r',
            b't',
            b'r',
            crate::ascii_defs::CAR,
        ]);
        assert_eq!(crate::input::typebuf_bytes_for_test(), expected);
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.cmd_silent);
    }

    #[test]
    fn execreg_line_continuation_joins_preceding_backslash_lines() {
        let lines = vec![
            b"echo 'a'".to_vec(),
            b"  \\ . 'b'".to_vec(),
            b"\\ . 'c'".to_vec(),
        ];
        let mut index = 2;

        let joined = execreg_line_continuation(&lines, &mut index);

        assert_eq!(joined, b"echo 'a' . 'b' . 'c'");
        assert_eq!(index, 0);
    }

    #[test]
    fn execreg_line_continuation_ignores_comment_continuations() {
        let lines = vec![
            b"echo 'a'".to_vec(),
            b"\"\\ ignored".to_vec(),
            b"\\ . 'b'".to_vec(),
        ];
        let mut index = 2;

        let joined = execreg_line_continuation(&lines, &mut index);

        assert_eq!(joined, b"echo 'a' . 'b'");
        assert_eq!(index, 0);
    }

    #[test]
    fn execreg_line_continuation_stops_at_the_nearest_command_start() {
        let lines = vec![
            b"echo 'old'".to_vec(),
            b"\\ . 'ignored'".to_vec(),
            b"echo 'new'".to_vec(),
            b"\\ . 'tail'".to_vec(),
        ];
        let mut index = 3;

        let joined = execreg_line_continuation(&lines, &mut index);

        assert_eq!(joined, b"echo 'new' . 'tail'");
        assert_eq!(index, 2);
    }

    // --- get_y_register / yank_register_mline ---

    #[test]
    fn get_y_register_bounds_checks_the_index() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            assert!(get_y_register(0).is_some());
            assert!(get_y_register(NUM_REGISTERS - 1).is_some());
            // The original indexes unchecked; this answers None.
            assert!(get_y_register(NUM_REGISTERS).is_none());
        }
    }

    #[test]
    fn get_y_register_returns_distinct_slots() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let a = get_y_register(0).unwrap();
            let b = get_y_register(1).unwrap();
            assert!(!std::ptr::eq(a, b));
        }
    }

    #[test]
    fn yank_register_mline_refuses_the_black_hole() {
        // Cross-verified against real nvim: getreg('_') is empty, so
        // the black hole can never be linewise.
        let _lock = crate::globals::global_state_test_lock();
        let (is_line, reg) = unsafe { yank_register_mline(i32::from(b'_')) };
        assert!(!is_line);
        assert!(reg.is_none(), "no register is handed back");
    }

    #[test]
    fn yank_register_mline_refuses_an_invalid_register_name() {
        let _lock = crate::globals::global_state_test_lock();
        let (is_line, reg) = unsafe { yank_register_mline(i32::from(b'!')) };
        assert!(!is_line);
        assert!(reg.is_none());
    }

    #[test]
    fn yank_register_mline_reports_the_registers_own_motion_type() {
        // Cross-verified against real nvim: `yy` leaves the register
        // linewise (getregtype is "V") while `yl` leaves it charwise
        // ("v").
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let slot = get_y_register(op_reg_index(i32::from(b'a')).unwrap()).unwrap();
            let saved = (*slot).y_type;

            (*slot).y_type = crate::normal_defs::MotionType::LineWise;
            let (is_line, reg) = yank_register_mline(i32::from(b'a'));
            assert!(is_line);
            assert!(reg.is_some());

            (*slot).y_type = crate::normal_defs::MotionType::CharWise;
            let (is_line, reg) = yank_register_mline(i32::from(b'a'));
            assert!(!is_line);
            assert!(reg.is_some(), "still handed back when not linewise");

            (*slot).y_type = saved;
        }
    }

    // --- op_reg_index / valid_yank_reg ---

    #[test]
    fn op_reg_index_digits() {
        assert_eq!(op_reg_index(i32::from(b'0')), Some(0));
        assert_eq!(op_reg_index(i32::from(b'9')), Some(9));
    }

    #[test]
    fn op_reg_index_letters_both_cases_map_to_the_same_slot() {
        assert_eq!(op_reg_index(i32::from(b'a')), Some(10));
        assert_eq!(op_reg_index(i32::from(b'z')), Some(35));
        assert_eq!(op_reg_index(i32::from(b'A')), Some(10));
        assert_eq!(op_reg_index(i32::from(b'Z')), Some(35));
    }

    #[test]
    fn op_reg_index_special_names() {
        assert_eq!(op_reg_index(i32::from(b'-')), Some(crate::register_defs::DELETION_REGISTER));
        assert_eq!(op_reg_index(i32::from(b'*')), Some(STAR_REGISTER));
        assert_eq!(op_reg_index(i32::from(b'+')), Some(PLUS_REGISTER));
    }

    #[test]
    fn op_reg_index_invalid_name_is_none() {
        assert_eq!(op_reg_index(i32::from(b'"')), None);
        assert_eq!(op_reg_index(0), None);
        assert_eq!(op_reg_index(i32::from(b'!')), None);
    }

    #[test]
    fn op_reg_get_returns_the_named_register_slot() {
        let _lock = crate::globals::global_state_test_lock();
        let idx = op_reg_index(i32::from(b'c')).unwrap();
        let expected = unsafe { get_y_register(idx) }.unwrap().cast_const();

        assert_eq!(unsafe { op_reg_get(i32::from(b'c')) }, Some(expected));
        assert_eq!(
            unsafe { op_reg_get(i32::from(b'C')) },
            Some(expected),
            "upper- and lower-case names share the same register"
        );
    }

    #[test]
    fn op_reg_get_returns_none_for_a_name_without_a_slot() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { op_reg_get(i32::from(b'!')) }, None);
        assert_eq!(unsafe { op_reg_get(i32::from(b'"')) }, None);
    }

    struct PreviousRegisterGuard(Option<usize>);

    impl PreviousRegisterGuard {
        fn save() -> Self {
            // SAFETY: the caller holds the global-state test lock.
            Self(unsafe { *Y_PREVIOUS.get_mut() })
        }
    }

    impl Drop for PreviousRegisterGuard {
        fn drop(&mut self) {
            // SAFETY: as in `save`.
            unsafe { *Y_PREVIOUS.get_mut() = self.0 };
        }
    }

    struct RegisterStateGuard {
        regs: Option<[YankregT; NUM_REGISTERS]>,
        previous: Option<usize>,
        execreg_lastc: i32,
        record_regname: i32,
    }

    impl RegisterStateGuard {
        fn save() -> Self {
            // SAFETY: the caller holds the global-state test lock.
            Self {
                regs: Some(unsafe { Y_REGS.get_mut() }.clone()),
                previous: unsafe { *Y_PREVIOUS.get_mut() },
                execreg_lastc: unsafe { *EXECR_LASTC.get_mut() },
                record_regname: unsafe { *RECORD_REGNAME.get_mut() },
            }
        }
    }

    impl Drop for RegisterStateGuard {
        fn drop(&mut self) {
            // SAFETY: as in `save`; moving the saved array restores all
            // owned register lines without cloning them again.
            unsafe {
                *Y_REGS.get_mut() = self.regs.take().expect("saved register state");
                *Y_PREVIOUS.get_mut() = self.previous;
                *EXECR_LASTC.get_mut() = self.execreg_lastc;
                *RECORD_REGNAME.get_mut() = self.record_regname;
            }
        }
    }

    #[test]
    fn do_execreg_queues_a_multiline_characterwise_register() {
        let _lock = crate::globals::global_state_test_lock();
        let _registers = RegisterStateGuard::save();
        let _typeahead = TypeaheadGuard::save();
        let _executing = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.reg_executing,
                0,
            )
        };
        let _pending = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.pending_end_reg_executing,
                true,
            )
        };
        let _restart = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.restart_edit,
                0,
            )
        };
        let index = op_reg_index(i32::from(b'a')).unwrap();
        unsafe {
            Y_REGS.get_mut()[index] = YankregT {
                y_array: Some(vec![b"one".to_vec(), b"two".to_vec()]),
                y_type: crate::normal_defs::MotionType::CharWise,
                ..Default::default()
            };
        }

        assert_eq!(
            unsafe { do_execreg(i32::from(b'a'), false, false, false) },
            crate::vim_defs::OK
        );

        assert_eq!(crate::input::typebuf_bytes_for_test(), b"one\ntwo");
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.reg_executing,
            i32::from(b'a')
        );
    }

    #[test]
    fn do_execreg_linewise_register_includes_a_final_newline() {
        let _lock = crate::globals::global_state_test_lock();
        let _registers = RegisterStateGuard::save();
        let _typeahead = TypeaheadGuard::save();
        let _executing = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.reg_executing,
                0,
            )
        };
        let _pending = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.pending_end_reg_executing,
                true,
            )
        };
        let _restart = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.restart_edit,
                0,
            )
        };
        let index = op_reg_index(i32::from(b'b')).unwrap();
        unsafe {
            Y_REGS.get_mut()[index] = YankregT {
                y_array: Some(vec![b"one".to_vec(), b"two".to_vec()]),
                y_type: crate::normal_defs::MotionType::LineWise,
                ..Default::default()
            };
        }

        assert_eq!(
            unsafe { do_execreg(i32::from(b'b'), false, false, false) },
            crate::vim_defs::OK
        );

        assert_eq!(crate::input::typebuf_bytes_for_test(), b"one\ntwo\n");
    }

    #[test]
    fn do_execreg_colon_mode_joins_continuations_and_prefixes_colon() {
        let _lock = crate::globals::global_state_test_lock();
        let _registers = RegisterStateGuard::save();
        let _typeahead = TypeaheadGuard::save();
        let _executing = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.reg_executing,
                0,
            )
        };
        let _pending = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.pending_end_reg_executing,
                true,
            )
        };
        let _restart = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.restart_edit,
                0,
            )
        };
        let index = op_reg_index(i32::from(b'c')).unwrap();
        unsafe {
            Y_REGS.get_mut()[index] = YankregT {
                y_array: Some(vec![
                    b"echo 'a'".to_vec(),
                    b"\\ . 'b'".to_vec(),
                ]),
                y_type: crate::normal_defs::MotionType::CharWise,
                ..Default::default()
            };
        }

        assert_eq!(
            unsafe { do_execreg(i32::from(b'c'), true, true, false) },
            crate::vim_defs::OK
        );

        assert_eq!(
            crate::input::typebuf_bytes_for_test(),
            b":echo 'a' . 'b'\n"
        );
    }

    #[test]
    fn do_execreg_repeats_the_last_register_and_accepts_black_hole() {
        let _lock = crate::globals::global_state_test_lock();
        let _registers = RegisterStateGuard::save();
        let _typeahead = TypeaheadGuard::save();
        let _executing = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.reg_executing,
                0,
            )
        };
        let _pending = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.pending_end_reg_executing,
                true,
            )
        };
        let _restart = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.restart_edit,
                0,
            )
        };
        let index = op_reg_index(i32::from(b'd')).unwrap();
        unsafe {
            Y_REGS.get_mut()[index] = YankregT {
                y_array: Some(vec![b"x".to_vec()]),
                ..Default::default()
            };
        }
        assert_eq!(
            unsafe { do_execreg(i32::from(b'd'), false, false, false) },
            crate::vim_defs::OK
        );
        assert_eq!(
            unsafe { do_execreg(i32::from(b'@'), false, false, false) },
            crate::vim_defs::OK
        );
        assert_eq!(
            unsafe { do_execreg(i32::from(b'_'), false, false, false) },
            crate::vim_defs::OK
        );

        assert_eq!(crate::input::typebuf_bytes_for_test(), b"xx");
    }

    #[test]
    fn do_execreg_queues_the_last_command_line() {
        let _lock = crate::globals::global_state_test_lock();
        let _registers = RegisterStateGuard::save();
        let _typeahead = TypeaheadGuard::save();
        let _cmdline =
            CmdlineGuard::set(Some(b"echo 1".to_vec()), Some(b"old".to_vec()));
        let _restart = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.restart_edit,
                0,
            )
        };

        assert_eq!(
            unsafe { do_execreg(i32::from(b':'), false, false, false) },
            crate::vim_defs::OK
        );

        assert_eq!(crate::input::typebuf_bytes_for_test(), b":echo 1\n");
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }
            .new_last_cmdline
            .is_none());
    }

    #[test]
    fn insert_reg_stuffs_characterwise_literal_register_text() {
        let _lock = crate::globals::global_state_test_lock();
        let _typeahead = TypeaheadGuard::save();
        let register = YankregT {
            y_array: Some(vec![vec![1, b'x']]),
            y_type: crate::normal_defs::MotionType::CharWise,
            ..Default::default()
        };

        assert_eq!(
            unsafe { insert_reg(i32::from(b'a'), Some(&register), false) },
            crate::vim_defs::OK
        );

        assert_eq!(
            stuffed_bytes(),
            vec![crate::ascii_defs::CTRL_V, 1, b'x']
        );
    }

    #[test]
    fn insert_reg_stuffs_linewise_registers_with_newlines() {
        let _lock = crate::globals::global_state_test_lock();
        let _typeahead = TypeaheadGuard::save();
        let register = YankregT {
            y_array: Some(vec![b"one".to_vec(), b"two".to_vec()]),
            y_type: crate::normal_defs::MotionType::LineWise,
            ..Default::default()
        };

        assert_eq!(
            unsafe { insert_reg(i32::from(b'!'), Some(&register), true) },
            crate::vim_defs::FAIL,
            "an invalid register name is rejected before using the supplied value"
        );
        assert_eq!(
            unsafe { insert_reg(i32::from(b'b'), Some(&register), false) },
            crate::vim_defs::OK
        );
        assert_eq!(stuffed_bytes(), b"one\ntwo\n");
    }

    #[test]
    fn insert_reg_stuffs_the_current_buffer_name_special_register() {
        let _lock = crate::globals::global_state_test_lock();
        let _typeahead = TypeaheadGuard::save();
        let mut buf = crate::buffer_defs::BufT {
            b_fname: Some(b"file.txt".to_vec()),
            ..Default::default()
        };
        let buf_ptr = std::ptr::addr_of_mut!(buf);
        let _curbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.curbuf,
                buf_ptr,
            )
        };

        assert_eq!(
            unsafe { insert_reg(i32::from(b'%'), None, false) },
            crate::vim_defs::OK
        );
        assert_eq!(stuffed_bytes(), b"file.txt");
    }

    #[test]
    fn insert_reg_uses_the_real_last_insert_for_dot() {
        let _lock = crate::globals::global_state_test_lock();
        let _typeahead = TypeaheadGuard::save();
        let _last_insert = LastInsertResetGuard;
        unsafe {
            crate::insert::reset_last_insert_for_test();
            crate::insert::set_last_insert(i32::from(b'x'));
        }

        assert_eq!(
            unsafe { insert_reg(i32::from(b'.'), None, false) },
            crate::vim_defs::OK
        );
        assert_eq!(stuffed_bytes(), b"x");
    }

    #[test]
    fn insert_reg_stops_on_interrupt_before_touching_buffers() {
        let _lock = crate::globals::global_state_test_lock();
        let _typeahead = TypeaheadGuard::save();
        let _interrupt = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.got_int,
                true,
            )
        };
        let register = YankregT {
            y_array: Some(vec![b"ignored".to_vec()]),
            ..Default::default()
        };

        assert_eq!(
            unsafe { insert_reg(i32::from(b'a'), Some(&register), false) },
            crate::vim_defs::FAIL
        );
        assert!(stuffed_bytes().is_empty());
    }

    #[test]
    #[should_panic(expected = "do_put")]
    fn insert_reg_small_delete_charwise_needs_real_buffer_editing() {
        let _lock = crate::globals::global_state_test_lock();
        let _typeahead = TypeaheadGuard::save();
        let register = YankregT {
            y_array: Some(vec![b"x".to_vec()]),
            y_type: crate::normal_defs::MotionType::CharWise,
            ..Default::default()
        };
        unsafe { insert_reg(i32::from(b'-'), Some(&register), false) };
    }

    #[test]
    fn op_reg_set_replaces_the_named_register() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = RegisterStateGuard::save();
        let value = YankregT {
            y_array: Some(vec![b"new value".to_vec()]),
            y_type: crate::normal_defs::MotionType::LineWise,
            y_width: 4,
            timestamp: 77,
        };

        assert!(unsafe { op_reg_set(i32::from(b'c'), value, false) });

        let reg = unsafe { &*op_reg_get(i32::from(b'c')).unwrap() };
        assert_eq!(reg.y_array.as_deref(), Some(&[b"new value".to_vec()][..]));
        assert_eq!(reg.y_type, crate::normal_defs::MotionType::LineWise);
        assert_eq!(reg.y_width, 4);
        assert_eq!(reg.timestamp, 77);
    }

    #[test]
    fn op_reg_set_updates_unnamed_fallback_only_when_requested() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = RegisterStateGuard::save();
        assert!(unsafe { op_reg_set_previous(i32::from(b'a')) });

        assert!(unsafe { op_reg_set(i32::from(b'b'), YankregT::default(), false) });
        assert_eq!(unsafe { *Y_PREVIOUS.get_mut() }, op_reg_index(i32::from(b'a')));

        assert!(unsafe { op_reg_set(i32::from(b'c'), YankregT::default(), true) });
        assert_eq!(unsafe { *Y_PREVIOUS.get_mut() }, op_reg_index(i32::from(b'c')));
    }

    #[test]
    fn op_reg_set_rejects_a_name_without_a_direct_slot() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = RegisterStateGuard::save();
        let before = unsafe { Y_REGS.get_mut()[0].clone() };

        assert!(!unsafe {
            op_reg_set(
                i32::from(b'!'),
                YankregT {
                    y_array: Some(vec![b"discarded".to_vec()]),
                    ..Default::default()
                },
                true,
            )
        });

        assert_eq!(
            unsafe { Y_REGS.get_mut()[0].y_array.as_ref() },
            before.y_array.as_ref()
        );
        assert_eq!(unsafe { *Y_PREVIOUS.get_mut() }, _g.previous);
    }

    #[test]
    fn op_reg_set_previous_selects_the_named_register() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = PreviousRegisterGuard::save();

        assert!(unsafe { op_reg_set_previous(i32::from(b'd')) });
        assert_eq!(
            unsafe { get_y_previous() }.cast_const(),
            unsafe { op_reg_get(i32::from(b'd')) }.unwrap()
        );
    }

    #[test]
    fn op_reg_set_previous_rejects_invalid_names_without_changing_selection() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = PreviousRegisterGuard::save();
        assert!(unsafe { op_reg_set_previous(i32::from(b'a')) });
        let before = unsafe { get_y_previous() };

        assert!(!unsafe { op_reg_set_previous(i32::from(b'!')) });
        assert_eq!(unsafe { get_y_previous() }, before);
    }

    #[test]
    fn free_register_releases_all_owned_lines() {
        let mut reg = YankregT {
            y_array: Some(vec![b"one".to_vec(), b"two".to_vec()]),
            ..Default::default()
        };

        free_register(&mut reg);

        assert!(reg.y_array.is_none());
    }

    #[test]
    fn free_register_preserves_register_metadata() {
        let mut reg = YankregT {
            y_array: Some(vec![b"text".to_vec()]),
            y_type: crate::normal_defs::MotionType::BlockWise,
            y_width: 7,
            timestamp: 123,
        };

        free_register(&mut reg);

        assert_eq!(reg.y_type, crate::normal_defs::MotionType::BlockWise);
        assert_eq!(reg.y_width, 7);
        assert_eq!(reg.timestamp, 123);
    }

    #[test]
    fn free_register_is_idempotent_for_an_empty_register() {
        let mut reg = YankregT::default();
        free_register(&mut reg);
        free_register(&mut reg);
        assert!(reg.y_array.is_none());
    }

    #[test]
    fn update_yankreg_width_uses_the_widest_blockwise_line() {
        let _lock = crate::globals::global_state_test_lock();
        let mut reg = YankregT {
            y_array: Some(vec![b"ab".to_vec(), b"hello".to_vec(), b"xyz".to_vec()]),
            y_type: crate::normal_defs::MotionType::BlockWise,
            ..Default::default()
        };

        unsafe { update_yankreg_width(&mut reg) };

        assert_eq!(reg.y_width, 4, "five cells are stored as width minus one");
    }

    #[test]
    fn update_yankreg_width_never_shrinks_an_existing_width() {
        let _lock = crate::globals::global_state_test_lock();
        let mut reg = YankregT {
            y_array: Some(vec![b"ab".to_vec()]),
            y_type: crate::normal_defs::MotionType::BlockWise,
            y_width: 9,
            ..Default::default()
        };

        unsafe { update_yankreg_width(&mut reg) };

        assert_eq!(reg.y_width, 9);
    }

    #[test]
    fn update_yankreg_width_ignores_non_blockwise_registers() {
        let _lock = crate::globals::global_state_test_lock();
        let mut reg = YankregT {
            y_array: Some(vec![b"very long line".to_vec()]),
            y_type: crate::normal_defs::MotionType::LineWise,
            y_width: 3,
            ..Default::default()
        };

        unsafe { update_yankreg_width(&mut reg) };

        assert_eq!(reg.y_width, 3);
    }

    #[test]
    fn update_yankreg_width_counts_multibyte_screen_cells() {
        let _lock = crate::globals::global_state_test_lock();
        let mut reg = YankregT {
            y_array: Some(vec!["一a".as_bytes().to_vec()]),
            y_type: crate::normal_defs::MotionType::BlockWise,
            ..Default::default()
        };

        unsafe { update_yankreg_width(&mut reg) };

        assert_eq!(reg.y_width, 2, "a double-width glyph plus ASCII occupies 3 cells");
    }

    #[test]
    fn shift_delete_registers_moves_one_through_eight_up_and_clears_one() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = RegisterStateGuard::save();
        let regs = unsafe { Y_REGS.get_mut() };
        for (n, reg) in regs.iter_mut().enumerate().take(10).skip(1) {
            *reg = YankregT {
                y_array: Some(vec![format!("r{n}").into_bytes()]),
                y_type: crate::normal_defs::MotionType::LineWise,
                y_width: n as i32,
                timestamp: 100 + n as u64,
            };
        }

        unsafe { shift_delete_registers(false) };

        let regs = unsafe { Y_REGS.get_mut() };
        assert!(regs[1].y_array.is_none());
        assert_eq!(regs[1].y_type, crate::normal_defs::MotionType::LineWise);
        assert_eq!(regs[1].y_width, 1, "the C code retains register 1 metadata");
        assert_eq!(regs[1].timestamp, 101);
        assert_eq!(regs[2].y_array.as_deref(), Some(&[b"r1".to_vec()][..]));
        assert_eq!(regs[9].y_array.as_deref(), Some(&[b"r8".to_vec()][..]));
        assert_eq!(unsafe { *Y_PREVIOUS.get_mut() }, Some(1));
    }

    #[test]
    fn shift_delete_registers_keeps_previous_when_appending() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = RegisterStateGuard::save();
        assert!(unsafe { op_reg_set_previous(i32::from(b'5')) });

        unsafe { shift_delete_registers(true) };

        assert_eq!(unsafe { *Y_PREVIOUS.get_mut() }, Some(5));
    }

    #[test]
    fn clear_registers_releases_every_registers_contents() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = RegisterStateGuard::save();
        let regs = unsafe { Y_REGS.get_mut() };
        for (idx, reg) in regs.iter_mut().enumerate() {
            reg.y_array = Some(vec![format!("slot {idx}").into_bytes()]);
        }

        unsafe { clear_registers() };

        assert!(unsafe { Y_REGS.get_mut() }.iter().all(|reg| reg.y_array.is_none()));
    }

    #[test]
    fn clear_registers_preserves_metadata_like_free_register() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = RegisterStateGuard::save();
        unsafe {
            Y_REGS.get_mut()[4] = YankregT {
                y_array: Some(vec![b"text".to_vec()]),
                y_type: crate::normal_defs::MotionType::BlockWise,
                y_width: 6,
                timestamp: 88,
            };
            clear_registers();
        }

        let reg = &unsafe { Y_REGS.get_mut() }[4];
        assert_eq!(reg.y_type, crate::normal_defs::MotionType::BlockWise);
        assert_eq!(reg.y_width, 6);
        assert_eq!(reg.timestamp, 88);
    }

    #[test]
    fn get_register_name_is_the_inverse_of_op_reg_index() {
        for regname in b'0'..=b'9' {
            let idx = op_reg_index(i32::from(regname)).unwrap();
            assert_eq!(get_register_name(idx as i32), i32::from(regname));
        }
        for regname in b'a'..=b'z' {
            let idx = op_reg_index(i32::from(regname)).unwrap();
            assert_eq!(get_register_name(idx as i32), i32::from(regname));
        }
        assert_eq!(get_register_name(crate::register_defs::DELETION_REGISTER as i32), i32::from(b'-'));
        assert_eq!(get_register_name(STAR_REGISTER as i32), i32::from(b'*'));
        assert_eq!(get_register_name(PLUS_REGISTER as i32), i32::from(b'+'));
    }

    #[test]
    fn get_register_name_of_minus_one_is_the_unnamed_register() {
        assert_eq!(get_register_name(-1), i32::from(b'"'));
    }

    #[test]
    fn get_unname_register_is_minus_one_when_y_previous_is_none() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { *Y_PREVIOUS.get_mut() }, None);
        assert_eq!(unsafe { get_unname_register() }, -1);
    }

    #[test]
    fn get_unname_register_matches_y_previous_when_set() {
        let _lock = crate::globals::global_state_test_lock();
        let saved = unsafe { *Y_PREVIOUS.get_mut() };
        unsafe { *Y_PREVIOUS.get_mut() = Some(5) };
        assert_eq!(unsafe { get_unname_register() }, 5);
        unsafe { *Y_PREVIOUS.get_mut() = saved };
    }

    #[test]
    fn valid_yank_reg_accepts_alphanumeric_and_the_documented_specials() {
        assert!(valid_yank_reg(i32::from(b'a'), false));
        assert!(valid_yank_reg(i32::from(b'Z'), false));
        assert!(valid_yank_reg(i32::from(b'5'), false));
        assert!(valid_yank_reg(i32::from(b'"'), false));
        assert!(valid_yank_reg(i32::from(b'-'), false));
        assert!(valid_yank_reg(i32::from(b'_'), false)); // black hole is valid
        assert!(valid_yank_reg(i32::from(b'*'), false));
        assert!(valid_yank_reg(i32::from(b'+'), false));
    }

    #[test]
    fn valid_yank_reg_accepts_readonly_specials_only_when_not_writing() {
        for &b in b"/#.%:=" {
            assert!(valid_yank_reg(i32::from(b), false), "{} should be valid when reading", b as char);
            assert!(!valid_yank_reg(i32::from(b), true), "{} should be invalid when writing", b as char);
        }
    }

    #[test]
    fn valid_yank_reg_rejects_other_punctuation() {
        assert!(!valid_yank_reg(i32::from(b'!'), false));
        assert!(!valid_yank_reg(i32::from(b'@'), false));
    }

    #[test]
    fn get_clipboard_always_fails() {
        assert!(!get_clipboard(i32::from(b'*')));
        assert!(!get_clipboard(i32::from(b'+')));
    }

    // --- get_yank_register ---

    #[test]
    fn get_yank_register_named_register_maps_to_its_own_slot() {
        let _lock = crate::globals::global_state_test_lock();
        let reg = unsafe { get_yank_register(i32::from(b'a'), YregModeT::Put) };
        // SAFETY: Y_REGS is a 'static array; this pointer is always valid.
        assert!(unsafe { (*reg).y_array.is_none() }, "register 'a' starts empty");
    }

    #[test]
    fn get_yank_register_unrecognized_name_falls_back_to_register_0() {
        let _lock = crate::globals::global_state_test_lock();
        // get_yank_register(0, Yank) below sets Y_PREVIOUS as a side
        // effect (matching the original's own real behavior) - reset
        // both before and after, matching every other Yank-mode test
        // in this file, so it doesn't leak into an unrelated test
        // asserting Y_PREVIOUS starts None (e.g.
        // get_unname_register_is_minus_one_when_y_previous_is_none).
        unsafe { *Y_PREVIOUS.get_mut() = None };
        let reg_invalid = unsafe { get_yank_register(i32::from(b'!'), YregModeT::Put) };
        let reg_zero = unsafe { get_yank_register(0, YregModeT::Yank) };
        assert_eq!(reg_invalid, reg_zero, "an unrecognized name uses register 0, matching the original");
        unsafe { *Y_PREVIOUS.get_mut() = None };
    }

    #[test]
    fn get_yank_register_star_and_plus_in_put_mode_return_the_empty_register_without_clipboard() {
        let _lock = crate::globals::global_state_test_lock();
        let reg_star = unsafe { get_yank_register(i32::from(b'*'), YregModeT::Put) };
        let reg_plus = unsafe { get_yank_register(i32::from(b'+'), YregModeT::Put) };
        assert_eq!(reg_star, unsafe { EMPTY_REG.get_mut() } as *mut YankregT);
        assert_eq!(reg_plus, unsafe { EMPTY_REG.get_mut() } as *mut YankregT);
    }

    #[test]
    fn get_yank_register_yank_mode_remembers_the_register_for_unnamed_paste() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *Y_PREVIOUS.get_mut() = None };

        let reg_a = unsafe { get_yank_register(i32::from(b'a'), YregModeT::Yank) };
        assert_eq!(unsafe { *Y_PREVIOUS.get_mut() }, Some(10));

        // Now an unnamed paste (mode != Yank, regname == 0) should
        // resolve to the same register 'a' just written.
        let reg_unnamed = unsafe { get_yank_register(0, YregModeT::Put) };
        assert_eq!(reg_a, reg_unnamed);

        unsafe { *Y_PREVIOUS.get_mut() = None };
    }

    // --- get_y_previous ---

    #[test]
    fn get_y_previous_is_null_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *Y_PREVIOUS.get_mut() = None };
        assert!(unsafe { get_y_previous() }.is_null());
    }

    #[test]
    fn get_y_previous_matches_the_register_yank_last_wrote_to() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *Y_PREVIOUS.get_mut() = None };

        let reg_a = unsafe { get_yank_register(i32::from(b'a'), YregModeT::Yank) };
        assert_eq!(unsafe { get_y_previous() }, reg_a);

        unsafe { *Y_PREVIOUS.get_mut() = None };
    }

    // --- "=" register (expr_line) ---

    #[test]
    fn expr_line_starts_unset() {
        let _lock = crate::globals::global_state_test_lock();
        set_expr_line(None);
        assert_eq!(get_expr_line_src(), None);
    }

    #[test]
    fn expr_line_src_returns_the_raw_text_unevaluated() {
        let _lock = crate::globals::global_state_test_lock();
        set_expr_line(Some(b"1 + 1".to_vec()));
        assert_eq!(get_expr_line_src(), Some(b"1 + 1".to_vec()));
        set_expr_line(None);
    }

    #[test]
    fn expr_line_evaluates_when_read_via_get_expr_line() {
        let _lock = crate::globals::global_state_test_lock();
        set_expr_line(Some(b"1 + 1".to_vec()));
        assert_eq!(unsafe { get_expr_line() }, Some(b"2".to_vec()));
        set_expr_line(None);
    }

    // --- get_spec_reg / get_reg_contents ---

    fn with_curbuf_curwin<R>(buf: &mut crate::buffer_defs::BufT, win: &mut crate::buffer_defs::WinT, f: impl FnOnce() -> R) -> R {
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_buf = globals.curbuf;
        let prev_win = globals.curwin;
        globals.curbuf = buf as *mut crate::buffer_defs::BufT;
        globals.curwin = win as *mut crate::buffer_defs::WinT;

        let result = f();

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.curbuf = prev_buf;
        globals.curwin = prev_win;
        result
    }

    #[test]
    fn get_spec_reg_percent_returns_the_current_buffer_file_name() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { b_fname: Some(b"foo.txt".to_vec()), ..Default::default() };
        let mut win = crate::buffer_defs::WinT::default();

        with_curbuf_curwin(&mut buf, &mut win, || {
            let result = unsafe { get_spec_reg(i32::from(b'%'), false) };
            assert_eq!(result, Some((Some(b"foo.txt".to_vec()), false)));
        });
    }

    #[test]
    fn get_spec_reg_hash_alternate_file_is_none_when_w_alt_fnum_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT { w_alt_fnum: 0, ..Default::default() };

        with_curbuf_curwin(&mut buf, &mut win, || {
            let result = unsafe { get_spec_reg(i32::from(b'#'), false) };
            assert_eq!(result, Some((None, false)));
        });
    }

    /// Points `GLOBALS.lastbuf` at `buf` for the guard's lifetime,
    /// restoring the previous value on drop - a `register.rs`-local
    /// copy of `buffer.rs`'s own private `LastbufGuard` (not directly
    /// reusable across files). Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime.
    struct LastbufGuard {
        previous: *mut crate::buffer_defs::BufT,
    }

    impl LastbufGuard {
        fn set(new_lastbuf: *mut crate::buffer_defs::BufT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.lastbuf;
            unsafe { crate::globals::GLOBALS.get_mut() }.lastbuf = new_lastbuf;
            LastbufGuard { previous }
        }
    }

    impl Drop for LastbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.lastbuf = self.previous;
        }
    }

    #[test]
    fn get_spec_reg_hash_alternate_file_resolves_the_real_alternate_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut alt_buf =
            crate::buffer_defs::BufT { handle: 5, b_fname: Some(b"alt.txt".to_vec()), ..Default::default() };
        let _lastbuf_guard = LastbufGuard::set(&mut alt_buf as *mut crate::buffer_defs::BufT);

        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT { w_alt_fnum: 5, ..Default::default() };

        let result = with_curbuf_curwin(&mut buf, &mut win, || unsafe { get_spec_reg(i32::from(b'#'), false) });
        assert_eq!(result, Some((Some(b"alt.txt".to_vec()), false)));
    }

    #[test]
    fn get_spec_reg_hash_alternate_file_is_none_for_an_unknown_alternate_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _lastbuf_guard = LastbufGuard::set(std::ptr::null_mut());
        let mut buf = crate::buffer_defs::BufT::default();
        // No buffer with handle 99 exists in the (empty) lastbuf list.
        let mut win = crate::buffer_defs::WinT { w_alt_fnum: 99, ..Default::default() };

        let result = with_curbuf_curwin(&mut buf, &mut win, || unsafe { get_spec_reg(i32::from(b'#'), false) });
        assert_eq!(result, Some((None, false)));
    }

    #[test]
    fn get_spec_reg_colon_returns_last_cmdline() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.last_cmdline.clone();
        unsafe { crate::globals::GLOBALS.get_mut() }.last_cmdline = Some(b":wq".to_vec());

        let result = unsafe { get_spec_reg(i32::from(b':'), false) };
        assert_eq!(result, Some((Some(b":wq".to_vec()), false)));

        unsafe { crate::globals::GLOBALS.get_mut() }.last_cmdline = prev;
    }

    #[test]
    fn get_spec_reg_slash_returns_last_search_pattern() {
        let _lock = crate::globals::global_state_test_lock();
        // No setter is exercised here (search.rs's own set_last_search_pat
        // needs more setup) - just confirms the unset default is None.
        let result = unsafe { get_spec_reg(i32::from(b'/'), false) };
        assert_eq!(result, Some((crate::search::last_search_pat(), false)));
    }

    #[test]
    fn get_spec_reg_dot_reports_nothing_before_any_insert() {
        // Nothing has called set_last_insert, so the last-insert
        // register is genuinely empty.
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::insert::reset_last_insert_for_test() };
        let result = unsafe { get_spec_reg(i32::from(b'.'), false) };
        assert_eq!(result, Some((None, true)));
    }

    #[test]
    fn get_spec_reg_dot_reports_the_real_last_insert() {
        // Once a real insert has been recorded, "@." reports it with
        // the trailing <Esc> removed.
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::insert::set_last_insert(i32::from(b'x')) };
        let result = unsafe { get_spec_reg(i32::from(b'.'), false) };
        unsafe { crate::insert::reset_last_insert_for_test() };
        assert_eq!(result, Some((Some(b"x".to_vec()), true)));
    }

    #[test]
    fn get_spec_reg_cursor_relative_registers_are_none_without_errmsg() {
        // Ctrl_F, Ctrl_P, Ctrl_W, Ctrl_A, Ctrl_L
        for &c in &[0x06, 0x10, 0x17, 0x01, 0x0c] {
            assert_eq!(unsafe { get_spec_reg(c, false) }, None, "0x{c:02x} should be None when !errmsg");
        }
    }

    #[test]
    fn get_spec_reg_cursor_relative_registers_are_unimplemented_with_errmsg() {
        let result = std::panic::catch_unwind(|| unsafe { get_spec_reg(0x17, true) }); // Ctrl_W
        assert!(result.is_err(), "expected a panic (find_ident_under_cursor not yet translated)");
    }

    #[test]
    fn get_spec_reg_black_hole_is_always_an_empty_string() {
        let result = unsafe { get_spec_reg(i32::from(b'_'), false) };
        assert_eq!(result, Some((Some(Vec::new()), false)));
    }

    #[test]
    fn get_spec_reg_unrecognized_name_is_none() {
        assert_eq!(unsafe { get_spec_reg(i32::from(b'a'), false) }, None);
    }

    #[test]
    fn get_reg_contents_equals_returns_the_source_text_with_expr_src_flag() {
        let _lock = crate::globals::global_state_test_lock();
        set_expr_line(Some(b"1 + 1".to_vec()));
        let result = unsafe { get_reg_contents(i32::from(b'='), greg_flags::EXPR_SRC) };
        assert_eq!(result, Some(RegContents::Str(b"1 + 1".to_vec())));
        set_expr_line(None);
    }

    #[test]
    fn get_reg_contents_equals_evaluates_without_expr_src_flag() {
        let _lock = crate::globals::global_state_test_lock();
        set_expr_line(Some(b"1 + 1".to_vec()));
        let result = unsafe { get_reg_contents(i32::from(b'='), 0) };
        assert_eq!(result, Some(RegContents::Str(b"2".to_vec())));
        set_expr_line(None);
    }

    #[test]
    fn get_reg_contents_equals_with_no_expr_flag_fails() {
        let result = unsafe { get_reg_contents(i32::from(b'='), greg_flags::NO_EXPR) };
        assert_eq!(result, None);
    }

    #[test]
    fn get_reg_contents_at_is_an_alias_for_the_unnamed_register() {
        let _lock = crate::globals::global_state_test_lock();
        let via_at = unsafe { get_reg_contents(i32::from(b'@'), greg_flags::EXPR_SRC) };
        let via_quote = unsafe { get_reg_contents(i32::from(b'"'), greg_flags::EXPR_SRC) };
        assert_eq!(via_at, via_quote);
    }

    #[test]
    fn get_reg_contents_invalid_name_is_none() {
        assert_eq!(unsafe { get_reg_contents(i32::from(b'!'), greg_flags::EXPR_SRC) }, None);
    }

    #[test]
    fn get_reg_contents_named_register_is_none_when_nothing_has_ever_yanked() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { get_reg_contents(i32::from(b'a'), greg_flags::EXPR_SRC) }, None);
    }

    #[test]
    fn get_reg_contents_black_hole_is_an_empty_string_not_none() {
        let result = unsafe { get_reg_contents(i32::from(b'_'), greg_flags::EXPR_SRC) };
        assert_eq!(result, Some(RegContents::Str(Vec::new())));
    }

    #[test]
    fn get_reg_contents_list_flag_wraps_a_special_register_in_a_one_element_list() {
        let _lock = crate::globals::global_state_test_lock();
        set_expr_line(Some(b"1 + 1".to_vec()));
        let result = unsafe { get_reg_contents(i32::from(b'='), greg_flags::EXPR_SRC | greg_flags::LIST) };
        match result {
            Some(RegContents::List(l)) => {
                assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 1);
                unsafe { crate::eval::typval::tv_list_free(l) };
            }
            other => panic!("expected a 1-element List, got {other:?}"),
        }
        set_expr_line(None);
    }

    #[test]
    fn get_reg_contents_list_flag_returns_one_item_per_yank_register_line() {
        let _lock = crate::globals::global_state_test_lock();
        let idx = op_reg_index(i32::from(b'f')).unwrap();
        unsafe {
            let reg = &mut Y_REGS.get_mut()[idx];
            reg.y_array = Some(vec![b"line one".to_vec(), b"line two".to_vec()]);
            reg.y_type = crate::normal_defs::MotionType::LineWise;
        }

        let result = unsafe { get_reg_contents(i32::from(b'f'), greg_flags::LIST) };
        match result {
            Some(RegContents::List(l)) => {
                assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 2);
                unsafe { crate::eval::typval::tv_list_free(l) };
            }
            other => panic!("expected a 2-element List, got {other:?}"),
        }

        unsafe { Y_REGS.get_mut()[idx] = YankregT::default() };
    }

    #[test]
    fn get_reg_contents_list_flag_is_none_for_an_unset_register() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { get_reg_contents(i32::from(b'g'), greg_flags::LIST) }, None);
    }


    // --- get_reg_type / format_reg_type ---

    #[test]
    fn get_reg_type_special_registers_are_always_charwise() {
        for regname in *b"%#=:/._" {
            assert_eq!(
                unsafe { get_reg_type(i32::from(regname), None) },
                Some(crate::normal_defs::MotionType::CharWise),
                "register {} should be charwise",
                regname as char
            );
        }
        // Ctrl_F / Ctrl_P / Ctrl_W / Ctrl_A.
        for regname in [0x06, 0x10, 0x17, 0x01] {
            assert_eq!(unsafe { get_reg_type(regname, None) }, Some(crate::normal_defs::MotionType::CharWise));
        }
    }

    #[test]
    fn get_reg_type_invalid_name_is_none() {
        assert_eq!(unsafe { get_reg_type(i32::from(b'!'), None) }, None);
    }

    #[test]
    fn get_reg_type_unset_named_register_is_none() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { get_reg_type(i32::from(b'a'), None) }, None);
    }

    #[test]
    fn get_reg_type_reads_a_populated_charwise_register() {
        let _lock = crate::globals::global_state_test_lock();
        let idx = op_reg_index(i32::from(b'b')).unwrap();
        unsafe {
            let reg = &mut Y_REGS.get_mut()[idx];
            reg.y_array = Some(vec![b"hi".to_vec()]);
            reg.y_type = crate::normal_defs::MotionType::CharWise;
        }

        assert_eq!(unsafe { get_reg_type(i32::from(b'b'), None) }, Some(crate::normal_defs::MotionType::CharWise));

        unsafe { Y_REGS.get_mut()[idx] = YankregT::default() };
    }

    #[test]
    fn get_reg_type_reads_a_populated_linewise_register() {
        let _lock = crate::globals::global_state_test_lock();
        let idx = op_reg_index(i32::from(b'c')).unwrap();
        unsafe {
            let reg = &mut Y_REGS.get_mut()[idx];
            reg.y_array = Some(vec![b"hi".to_vec()]);
            reg.y_type = crate::normal_defs::MotionType::LineWise;
        }

        assert_eq!(unsafe { get_reg_type(i32::from(b'c'), None) }, Some(crate::normal_defs::MotionType::LineWise));

        unsafe { Y_REGS.get_mut()[idx] = YankregT::default() };
    }

    #[test]
    fn get_reg_type_blockwise_register_populates_reg_width() {
        let _lock = crate::globals::global_state_test_lock();
        let idx = op_reg_index(i32::from(b'd')).unwrap();
        unsafe {
            let reg = &mut Y_REGS.get_mut()[idx];
            reg.y_array = Some(vec![b"hi".to_vec()]);
            reg.y_type = crate::normal_defs::MotionType::BlockWise;
            reg.y_width = 3;
        }

        let mut width: crate::pos_defs::ColnrT = -1;
        let reg_type = unsafe { get_reg_type(i32::from(b'd'), Some(&mut width)) };
        assert_eq!(reg_type, Some(crate::normal_defs::MotionType::BlockWise));
        assert_eq!(width, 3);

        unsafe { Y_REGS.get_mut()[idx] = YankregT::default() };
    }

    #[test]
    fn get_reg_type_none_reg_width_is_accepted_for_a_blockwise_register() {
        let _lock = crate::globals::global_state_test_lock();
        let idx = op_reg_index(i32::from(b'e')).unwrap();
        unsafe {
            let reg = &mut Y_REGS.get_mut()[idx];
            reg.y_array = Some(vec![b"hi".to_vec()]);
            reg.y_type = crate::normal_defs::MotionType::BlockWise;
            reg.y_width = 3;
        }

        assert_eq!(unsafe { get_reg_type(i32::from(b'e'), None) }, Some(crate::normal_defs::MotionType::BlockWise));

        unsafe { Y_REGS.get_mut()[idx] = YankregT::default() };
    }

    #[test]
    fn format_reg_type_charwise_and_linewise() {
        assert_eq!(format_reg_type(Some(crate::normal_defs::MotionType::CharWise), 0), b"v");
        assert_eq!(format_reg_type(Some(crate::normal_defs::MotionType::LineWise), 0), b"V");
    }

    #[test]
    fn format_reg_type_blockwise_includes_ctrl_v_and_width_plus_one() {
        let result = format_reg_type(Some(crate::normal_defs::MotionType::BlockWise), 3);
        assert_eq!(result, b"\x164");
    }

    #[test]
    fn format_reg_type_unknown_is_empty() {
        assert_eq!(format_reg_type(None, 0), Vec::<u8>::new());
    }
}
