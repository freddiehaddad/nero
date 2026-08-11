//! `src/nvim/grid.c` - the screen grid and its glyph (`schar_T`) encoding.
//!
//! Only the `schar_T` encoding half of this file is translated so far:
//! the interning cache plus the small conversion helpers around it,
//! which `optionstr.c`'s `set_chars_option` needs to turn a
//! `'listchars'`/`'fillchars'` character into a `schar_T`.
//!
//! A `schar_T` is a `u32` holding either the glyph's own UTF-8 bytes
//! inline (up to four of them, NUL-padded) or, for anything longer, a
//! marker byte plus an index into the process-wide glyph cache. The
//! discriminator is the FIRST byte in memory order: `0xFF` means
//! "interned". The original spells that out as two `#ifdef
//! ORDER_BIG_ENDIAN` branches (`0xFF + (idx << 8)` on little-endian,
//! `idx + (0xFF << 24)` on big-endian) precisely because both encode
//! the same memory-order layout; this translation keeps the same two
//! branches rather than collapsing them, so the bit patterns stay
//! identical on both endiannesses.
//!
//! Also `grid_clear_line`, `grid_invalidate` and `grid_getchar` - the
//! three `ScreenGrid` accessors that need only the grid's own backing
//! arrays, not the redraw pipeline. `grid_getchar` returns
//! `(schar, attr)` rather than taking the original's optional `attrp`
//! out-parameter.
//!
//! Deferred (each genuinely blocked, not simply "not gotten to yet"):
//! everything that actually draws - `grid_line_start`/`grid_line_puts`/
//! `grid_put_linebuf`/`grid_scroll`/`grid_clear` and the `ScreenGrid`
//! allocation helpers - needs the UI event pipeline (`ui_line`,
//! `ui_call_grid_scroll`), the highlight attribute registry
//! (`hl_combine_attr`), and `decor`'s own providers, none translated.
//! `schar_cache_clear` likewise needs `decor_check_invalid_glyphs`,
//! and the private `grid_invalid_row` has no caller but
//! `grid_put_linebuf`, so it is held back rather than landed as dead
//! code.
//!
//! `line_do_arabic_shape` and its two private helpers landed once
//! `arabic.rs`'s own `arabic_shape` did. Note the argument order at
//! its `arabic_shape` call: the NEXT character is passed as that
//! function's `prev_c`, and the PREVIOUS one as its `next_c`. That is
//! not a mistake - the glyph buffer is in VISUAL order while
//! `arabic_shape` reasons in LOGICAL order, and Arabic runs
//! right-to-left, so the two are mirrored.
//!
//! `schar_get_adv` is deliberately NOT translated: the original's
//! `schar_get`/`schar_get_adv` split exists purely so a caller can
//! append into a larger scratch buffer without a second copy, and
//! [`schar_get`] here already returns the bytes owned.

use crate::map::Set;
use crate::types_defs::{MAX_SCHAR_SIZE, ScharT};

/// The process-wide glyph interning cache (`glyph_cache`).
///
/// The original is a `Set(glyph)` of NUL-terminated strings; here the
/// keys are plain byte vectors, since a `schar_T`'s own glyph may not
/// contain embedded NULs anyway.
pub static GLYPH_CACHE: std::sync::LazyLock<crate::globals::GlobalCell<Set<Vec<u8>>>> =
    std::sync::LazyLock::new(|| crate::globals::GlobalCell::new(Set::default()));

/// Resolve a view's own row/column offsets against its target grid
/// (`grid_adjust`).
///
/// When the default grid is used, window-relative positions become
/// global screen positions.
///
/// @return `(target, row_off, col_off)` - the grid actually drawn to,
///         plus the adjusted offsets. The original adds into `int *`
///         in-out parameters; they are taken and returned by value
///         here.
#[must_use]
pub fn grid_adjust(
    grid: &crate::grid_defs::GridView,
    row_off: i32,
    col_off: i32,
) -> (*mut crate::grid_defs::ScreenGrid, i32, i32) {
    (grid.target, row_off + grid.row_offset, col_off + grid.col_offset)
}

/// Whether `row` of `grid` has been invalidated (`grid_invalid_row`).
///
/// A negative attribute in the row's first cell is the marker
/// `grid_invalidate` leaves behind.
///
/// # Safety
/// `grid.attrs` and `grid.line_offset` must be valid, allocated arrays
/// with at least `row + 1` rows.
#[must_use]
pub unsafe fn grid_invalid_row(grid: &crate::grid_defs::ScreenGrid, row: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let off = unsafe { *grid.line_offset.add(row as usize) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *grid.attrs.add(off) < 0 }
}

/// The starting column for border text of a given width
/// (`get_bordertext_col`).
///
/// Columns are 1-based, and the result never falls below 1: text wider
/// than the border it sits on starts flush at the left rather than
/// running off it.
#[must_use]
pub fn get_bordertext_col(
    total_col: i32,
    text_width: i32,
    align: crate::buffer_defs::AlignTextPos,
) -> i32 {
    match align {
        crate::buffer_defs::AlignTextPos::Left => 1,
        crate::buffer_defs::AlignTextPos::Center => ((total_col - text_width) / 2 + 1).max(1),
        crate::buffer_defs::AlignTextPos::Right => (total_col - text_width + 1).max(1),
    }
}

/// Line buffer holding the characters to be drawn (`linebuf_char`).
///
/// The original is a raw pointer to an array reallocated as the screen
/// resizes; a `Vec` carries its own length, so the separate size
/// tracking has no equivalent.
pub static LINEBUF_CHAR: crate::globals::GlobalCell<Vec<ScharT>> =
    crate::globals::GlobalCell::new(Vec::new());
/// Line buffer holding each cell's highlight attribute
/// (`linebuf_attr`).
pub static LINEBUF_ATTR: crate::globals::GlobalCell<Vec<crate::types_defs::SattrT>> =
    crate::globals::GlobalCell::new(Vec::new());
/// Line buffer holding each cell's virtual column (`linebuf_vcol`).
pub static LINEBUF_VCOL: crate::globals::GlobalCell<Vec<crate::pos_defs::ColnrT>> =
    crate::globals::GlobalCell::new(Vec::new());
/// Scratch line buffer (`linebuf_scratch`).
pub static LINEBUF_SCRATCH: crate::globals::GlobalCell<Vec<u8>> =
    crate::globals::GlobalCell::new(Vec::new());

static GRID_LINE_FIRST: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(i32::MAX);
static GRID_LINE_LAST: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);
static GRID_LINE_CLEAR_TO: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);
static GRID_LINE_BG_ATTR: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);
static GRID_LINE_CLEAR_ATTR: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);
static GRID_LINE_FLAGS: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);
static GRID_LINE_MAXCOL: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);
static GRID_LINE_GRID: crate::globals::GlobalCell<*mut crate::grid_defs::ScreenGrid> =
    crate::globals::GlobalCell::new(std::ptr::null_mut());
static GRID_LINE_ROW: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(-1);
static GRID_LINE_COLOFF: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);

/// `grid_put_linebuf` flag marking right-to-left text
/// (`SLF_RIGHTLEFT`).
pub const SLF_RIGHTLEFT: i32 = 1;

/// Starts buffering one row of `grid` (`screengrid_line_start`).
///
/// # Safety
/// `grid` must remain live until the matching flush/end operation.
/// No other grid line may already be active.
pub unsafe fn screengrid_line_start(
    grid: *mut crate::grid_defs::ScreenGrid,
    row: i32,
    col: i32,
) {
    assert!(!grid.is_null());
    assert!(unsafe { *GRID_LINE_GRID.get_mut() }.is_null());
    let linebuf_size = unsafe { (*LINEBUF_CHAR.get_mut()).len() };
    let grid_cols = unsafe { (*grid).cols };
    let maxcol = grid_cols.min(grid_cols - col);
    assert!(usize::try_from(maxcol).is_ok_and(|n| n <= linebuf_size));

    unsafe {
        *GRID_LINE_ROW.get_mut() = row;
        *GRID_LINE_GRID.get_mut() = grid;
        *GRID_LINE_COLOFF.get_mut() = col;
        *GRID_LINE_FIRST.get_mut() =
            i32::try_from(linebuf_size).expect("line buffer length must fit i32");
        *GRID_LINE_MAXCOL.get_mut() = maxcol;
        *GRID_LINE_LAST.get_mut() = 0;
        *GRID_LINE_CLEAR_TO.get_mut() = 0;
        *GRID_LINE_BG_ATTR.get_mut() = 0;
        *GRID_LINE_CLEAR_ATTR.get_mut() = 0;
        *GRID_LINE_FLAGS.get_mut() = 0;
    }

    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    let rdb_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.rdb_flags;
    if globals.full_screen
        && rdb_flags & crate::option_vars::opt_rdb_flag::INVALID != 0
    {
        unsafe {
            LINEBUF_CHAR.get_mut().fill(ScharT::MAX);
            LINEBUF_ATTR.get_mut().fill(-1);
        }
    }
}

/// Starts buffering one row through a possibly offset `GridView`
/// (`grid_line_start`).
///
/// # Safety
/// The view's target grid must satisfy
/// [`screengrid_line_start`]'s lifetime requirements.
pub unsafe fn grid_line_start(view: &crate::grid_defs::GridView, row: i32) {
    let (grid, row, col) = grid_adjust(view, row, 0);
    unsafe { screengrid_line_start(grid, row, col) };
}

/// Reads the cell currently displayed at `col` of the active grid
/// line (`grid_line_getchar`).
///
/// The pending line buffer is not consulted. Out-of-range columns
/// return a neutral space and no attribute, matching the original's
/// untouched optional attribute out-parameter.
///
/// # Safety
/// An active grid line must exist, and its backing arrays must cover
/// the active row and adjusted column.
#[must_use]
pub unsafe fn grid_line_getchar(
    col: i32,
) -> (ScharT, Option<crate::types_defs::SattrT>) {
    if col >= unsafe { *GRID_LINE_MAXCOL.get_mut() } {
        return (schar_from_ascii(b' '), None);
    }
    let grid = unsafe { *GRID_LINE_GRID.get_mut() };
    assert!(!grid.is_null());
    let adjusted = col + unsafe { *GRID_LINE_COLOFF.get_mut() };
    let row = unsafe { *GRID_LINE_ROW.get_mut() };
    let off = unsafe {
        *(*grid).line_offset.add(row as usize) + adjusted as usize
    };
    unsafe {
        (
            *(*grid).chars.add(off),
            Some(*(*grid).attrs.add(off)),
        )
    }
}

/// Marks the remainder of the buffered grid line for clearing
/// (`grid_line_clear_end`).
///
/// `bg_attr` applies to both buffered cells and cleared columns;
/// `clear_attr` only to the columns being cleared.
pub fn grid_line_clear_end(
    start_col: i32,
    end_col: i32,
    bg_attr: i32,
    clear_attr: i32,
) {
    unsafe {
        if *GRID_LINE_FIRST.get_mut() > start_col {
            *GRID_LINE_FIRST.get_mut() = start_col;
            *GRID_LINE_LAST.get_mut() = start_col;
        }
        *GRID_LINE_CLEAR_TO.get_mut() = end_col;
        *GRID_LINE_BG_ATTR.get_mut() = bg_attr;
        *GRID_LINE_CLEAR_ATTR.get_mut() = clear_attr;
    }
}

/// Fills a range in the currently buffered grid line
/// (`grid_line_fill`) and returns the clamped end column.
///
/// # Safety
/// The global line buffers must contain at least
/// `GRID_LINE_MAXCOL` cells.
pub unsafe fn grid_line_fill(
    start_col: i32,
    end_col: i32,
    sc: ScharT,
    attr: i32,
) -> i32 {
    let end_col = end_col.min(unsafe { *GRID_LINE_MAXCOL.get_mut() });
    if start_col >= end_col {
        return end_col;
    }
    let start = usize::try_from(start_col).expect("start_col must be nonnegative");
    let end = usize::try_from(end_col).expect("end_col must be nonnegative");
    for col in start..end {
        unsafe {
            (&mut *LINEBUF_CHAR.get_mut())[col] = sc;
            (&mut *LINEBUF_ATTR.get_mut())[col] = attr;
            (&mut *LINEBUF_VCOL.get_mut())[col] = -1;
        }
    }
    unsafe {
        *GRID_LINE_FIRST.get_mut() = (*GRID_LINE_FIRST.get_mut()).min(start_col);
        *GRID_LINE_LAST.get_mut() = (*GRID_LINE_LAST.get_mut()).max(end_col);
    }
    end_col
}

/// Writes one glyph and attribute into the current buffered grid line
/// (`grid_line_put_schar`).
///
/// # Safety
/// A grid line must have been started (`GRID_LINE_GRID` non-null), and
/// all global line buffers must have at least `GRID_LINE_MAXCOL`
/// entries.
pub unsafe fn grid_line_put_schar(col: i32, schar: ScharT, attr: i32) {
    assert!(!unsafe { *GRID_LINE_GRID.get_mut() }.is_null());
    if col >= unsafe { *GRID_LINE_MAXCOL.get_mut() } {
        return;
    }
    let col_idx = usize::try_from(col).expect("col must be nonnegative");
    unsafe {
        (&mut *LINEBUF_CHAR.get_mut())[col_idx] = schar;
        (&mut *LINEBUF_ATTR.get_mut())[col_idx] = attr;
        (&mut *LINEBUF_VCOL.get_mut())[col_idx] = -1;
        *GRID_LINE_FIRST.get_mut() = (*GRID_LINE_FIRST.get_mut()).min(col);
        *GRID_LINE_LAST.get_mut() = (*GRID_LINE_LAST.get_mut()).max(col + 1);
    }
}

/// Mirrors a buffered line range for right-to-left drawing
/// (`linebuf_mirror`).
///
/// Character, attribute and virtual-column buffers are mirrored
/// together. A two-cell character (`glyph, 0`) remains in that order
/// after moving to its mirrored position.
///
/// # Safety
/// Mutates the global `LINEBUF_*` arrays.
///
/// # Panics
/// Panics when the supplied range/width do not describe valid indices
/// in all three line buffers, matching the original's caller
/// invariants.
pub unsafe fn linebuf_mirror(
    firstp: &mut i32,
    lastp: &mut i32,
    clearp: &mut i32,
    width: i32,
) {
    let first = usize::try_from(*firstp).expect("first must be nonnegative");
    let last = usize::try_from(*lastp).expect("last must be nonnegative");
    let width_usize = usize::try_from(width).expect("width must be nonnegative");
    assert!(first <= last && last <= width_usize);

    let scratch_char = unsafe { &*LINEBUF_CHAR.get_mut() }[first..last].to_vec();
    let scratch_attr = unsafe { &*LINEBUF_ATTR.get_mut() }[first..last].to_vec();
    let scratch_vcol = unsafe { &*LINEBUF_VCOL.get_mut() }[first..last].to_vec();
    let mirror = width_usize - 1;

    let chars = unsafe { &mut *LINEBUF_CHAR.get_mut() };
    let mut col = first;
    while col < last {
        let rev = mirror - col;
        let source = scratch_char[col - first];
        if col + 1 < last && scratch_char[col + 1 - first] == 0 {
            chars[rev - 1] = source;
            chars[rev] = 0;
            col += 2;
        } else {
            chars[rev] = source;
            col += 1;
        }
    }

    let attrs = unsafe { &mut *LINEBUF_ATTR.get_mut() };
    let vcols = unsafe { &mut *LINEBUF_VCOL.get_mut() };
    for col in first..last {
        attrs[mirror - col] = scratch_attr[col - first];
        vcols[mirror - col] = scratch_vcol[col - first];
    }

    let old_first = *firstp;
    let old_last = *lastp;
    let old_clear = *clearp;
    *firstp = width - old_clear;
    *clearp = width - old_first;
    *lastp = width - old_last;
}

/// Mirrors the current buffered grid line when it contains output
/// (`grid_line_mirror`).
///
/// The clear range is first extended through the last buffered cell;
/// an empty range returns without touching buffers or flags.
///
/// # Safety
/// Same global line-buffer requirements as [`linebuf_mirror`].
pub unsafe fn grid_line_mirror(width: i32) {
    let clear_to = unsafe {
        (*GRID_LINE_CLEAR_TO.get_mut()).max(*GRID_LINE_LAST.get_mut())
    };
    unsafe { *GRID_LINE_CLEAR_TO.get_mut() = clear_to };
    if unsafe { *GRID_LINE_FIRST.get_mut() } >= clear_to {
        return;
    }

    let first = unsafe { GRID_LINE_FIRST.get_mut() };
    let last = unsafe { GRID_LINE_LAST.get_mut() };
    let clear = unsafe { GRID_LINE_CLEAR_TO.get_mut() };
    unsafe { linebuf_mirror(first, last, clear, width) };
    unsafe { *GRID_LINE_FLAGS.get_mut() |= SLF_RIGHTLEFT };
}

/// Whether the cell at `col` differs from what the grid already shows
/// (`grid_char_needs_redraw`).
///
/// A redraw is needed when the character or its attribute changed, or
/// when the character is two cells wide and its second cell differs.
/// Ex mode and `'redrawdebug'`'s "nodelta" both force every cell to be
/// redrawn regardless.
///
/// # Safety
/// `grid.chars`/`grid.attrs` must be valid arrays covering `off_to`
/// (and `off_to + 1` when `cols > 1`), and reads `GLOBALS`/
/// `OPTION_VARS` plus the `LINEBUF_*` file-statics.
#[must_use]
pub unsafe fn grid_char_needs_redraw(
    grid: &crate::grid_defs::ScreenGrid,
    col: usize,
    off_to: usize,
    cols: i32,
) -> bool {
    if cols <= 0 {
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let (linebuf_char, linebuf_attr) =
        unsafe { (&*LINEBUF_CHAR.get_mut(), &*LINEBUF_ATTR.get_mut()) };

    let ch = linebuf_char.get(col).copied().unwrap_or(0);
    let at = linebuf_attr.get(col).copied().unwrap_or(0);
    // SAFETY: forwarded from this function's own safety doc.
    let (grid_ch, grid_at) = unsafe { (*grid.chars.add(off_to), *grid.attrs.add(off_to)) };

    let mut changed = ch != grid_ch || at != grid_at;

    if !changed && cols > 1 {
        // A double-width character's second cell is stored as 0; if
        // the grid disagrees there, the pair still needs redrawing.
        let next = linebuf_char.get(col + 1).copied().unwrap_or(0);
        // SAFETY: forwarded from this function's own safety doc.
        let grid_next = unsafe { *grid.chars.add(off_to + 1) };
        changed = next == 0 && next != grid_next;
    }

    if changed {
        return true;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let exmode_active = unsafe { crate::globals::GLOBALS.get_mut() }.exmode_active;
    // SAFETY: forwarded from this function's own safety doc.
    let rdb_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.rdb_flags;

    exmode_active || rdb_flags & crate::option_vars::opt_rdb_flag::NODELTA != 0
}

/// Handle for the default grid (`DEFAULT_GRID_HANDLE`).
pub const DEFAULT_GRID_HANDLE: crate::types_defs::HandleT = 1;

/// `schar_from_ascii(x)` - a single ASCII byte as a `schar_T`.
///
/// The original is a macro with two endianness branches, deliberately
/// written so a compile-time-constant argument stays compile-time
/// constant; `const fn` gives the same guarantee here.
#[must_use]
pub const fn schar_from_ascii(x: u8) -> ScharT {
    if cfg!(target_endian = "big") {
        (x as ScharT) << 24
    } else {
        x as ScharT
    }
}

/// The most recently assigned grid handle (`last_grid_handle`, a
/// function-static in the original).
static LAST_GRID_HANDLE: crate::globals::GlobalCell<crate::types_defs::HandleT> =
    crate::globals::GlobalCell::new(DEFAULT_GRID_HANDLE);

/// Give `grid` a handle, unless it already has one
/// (`grid_assign_handle`).
///
/// The grid need not be allocated. Handles are assigned from a single
/// increasing counter, so the same grid keeps its identity across
/// reallocation.
///
/// # Safety
/// Mutates the `LAST_GRID_HANDLE` file-static.
pub unsafe fn grid_assign_handle(grid: &mut crate::grid_defs::ScreenGrid) {
    if grid.handle == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let next = unsafe { LAST_GRID_HANDLE.get_mut() };
        *next += 1;
        grid.handle = *next;
    }
}

/// The window whose own allocated grid carries `handle`, in the
/// current tabpage (`get_win_by_grid_handle`).
///
/// # Safety
/// `GLOBALS.firstwin` and its `w_next` chain must be valid pointers to
/// live `WinT`s.
#[must_use]
pub unsafe fn get_win_by_grid_handle(
    handle: crate::types_defs::HandleT,
) -> *mut crate::buffer_defs::WinT {
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { (*wp).w_grid_alloc.handle } == handle {
            return wp;
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { (*wp).w_next };
    }
    std::ptr::null_mut()
}

/// Whether `sc` refers to the glyph cache rather than holding its own
/// bytes inline (`schar_high`).
#[must_use]
pub const fn schar_high(sc: ScharT) -> bool {
    if cfg!(target_endian = "big") {
        (sc & 0xFF00_0000) == 0xFF00_0000
    } else {
        (sc & 0xFF) == 0xFF
    }
}

/// The glyph-cache index carried by a `schar_high` value
/// (`schar_idx`).
#[must_use]
pub const fn schar_idx(sc: ScharT) -> u32 {
    if cfg!(target_endian = "big") {
        sc & 0x00FF_FFFF
    } else {
        sc >> 8
    }
}

/// `schar_from_buf(const char *buf, size_t len)`
///
/// `buf` need not be NUL terminated, but may not contain embedded
/// NULs. The original asserts `len < MAX_SCHAR_SIZE` (strictly less,
/// as a NUL still needs a byte); that assert is kept as a
/// `debug_assert!`, matching how the original's own `assert` compiles
/// out of release builds.
///
/// # Panics
/// In debug builds, if `buf` is `MAX_SCHAR_SIZE` bytes or longer, or
/// if the glyph cache has grown past its own `0xFFFFFF` index limit.
#[must_use]
pub fn schar_from_buf(buf: &[u8]) -> ScharT {
    debug_assert!(buf.len() < MAX_SCHAR_SIZE);
    if buf.len() <= 4 {
        // The original `memcpy`s straight into the `schar_T`, so the
        // bytes land in memory order and the remainder stays zero.
        let mut bytes = [0u8; 4];
        bytes[..buf.len()].copy_from_slice(buf);
        return ScharT::from_ne_bytes(bytes);
    }

    // SAFETY: a plain global-cell borrow, no aliasing hazard.
    let cache = unsafe { GLYPH_CACHE.get_mut() };
    let (idx, _status) = cache.put(buf.to_vec());
    let idx = u32::try_from(idx).expect("glyph cache index fits in u32");
    debug_assert!(idx < 0xFF_FFFF);
    if cfg!(target_endian = "big") {
        idx + (0xFF << 24)
    } else {
        0xFF + (idx << 8)
    }
}

/// `schar_from_char(int c)` - a single codepoint as a `schar_T`.
///
/// The original clamps anything at or above `0x200000` to `U+FFFD`
/// with a "this must NEVER happen, even if the file contained overlong
/// sequences" note, and encodes straight into the `schar_T`'s own four
/// bytes - a codepoint below that limit is at most four UTF-8 bytes,
/// so it always fits inline and never touches the glyph cache.
#[must_use]
pub fn schar_from_char(c: i32) -> ScharT {
    let c = if c >= 0x0020_0000 { 0xFFFD } else { c };
    let mut bytes = [0u8; 4];
    let len = crate::mbyte::utf_char2bytes(c, &mut bytes);
    let len = usize::try_from(len).unwrap_or(0).min(4);
    ScharT::from_ne_bytes({
        let mut b = [0u8; 4];
        b[..len].copy_from_slice(&bytes[..len]);
        b
    })
}

/// `schar_from_str(const char *str)`
///
/// `None` models the original's own `NULL` argument, which yields `0`
/// (`set_chars_option`'s own `chars_tab` entries rely on this for
/// their optional `def`/`fallback` strings).
#[must_use]
pub fn schar_from_str(str: Option<&[u8]>) -> ScharT {
    match str {
        None => 0,
        Some(s) => schar_from_buf(s),
    }
}

/// The number of bytes in `sc`'s own glyph (`schar_len`).
#[must_use]
pub fn schar_len(sc: ScharT) -> usize {
    if schar_high(sc) {
        // SAFETY: a plain global-cell borrow, no aliasing hazard.
        let cache = unsafe { GLYPH_CACHE.get_mut() };
        let idx = schar_idx(sc) as usize;
        debug_assert!(idx < cache.len());
        cache.keys().get(idx).map_or(0, Vec::len)
    } else {
        let bytes = sc.to_ne_bytes();
        bytes.iter().position(|&b| b == 0).unwrap_or(4)
    }
}

/// `sc`'s glyph bytes with a trailing NUL, as the original's own
/// `char sc_buf[MAX_SCHAR_SIZE]` scratch buffer always has.
///
/// `utf_ptr2char`/`utf_ptr2cells` both read a NUL-terminated buffer
/// and rely on that terminator to stop; [`schar_get`] returns just the
/// glyph, so callers that hand the bytes to those two must re-add it.
/// Without the terminator an empty glyph would index an empty slice.
fn schar_get_nul_terminated(sc: ScharT) -> Vec<u8> {
    let mut buf = schar_get(sc);
    buf.push(0);
    buf
}

/// How many screen cells `sc` occupies (`schar_cells`).
#[must_use]
pub fn schar_cells(sc: ScharT) -> i32 {
    // hot path
    let ascii = if cfg!(target_endian = "big") {
        (sc & 0x80FF_FFFF) == 0
    } else {
        sc < 0x80
    };
    if ascii {
        return 1;
    }

    // SAFETY: the buffer is NUL-terminated, which is exactly what
    // `utf_ptr2cells` needs to stop.
    unsafe { crate::mbyte::utf_ptr2cells(&schar_get_nul_terminated(sc)) }
}

/// `sc`'s first codepoint (`schar_get_first_codepoint`).
#[must_use]
pub fn schar_get_first_codepoint(sc: ScharT) -> i32 {
    crate::mbyte::utf_ptr2char(&schar_get_nul_terminated(sc))
}

/// `sc` as an ASCII byte, or NUL when it is not ASCII
/// (`schar_get_ascii`).
#[must_use]
pub const fn schar_get_ascii(sc: ScharT) -> u8 {
    if cfg!(target_endian = "big") {
        if (sc & 0x80FF_FFFF) == 0 {
            (sc >> 24) as u8
        } else {
            0
        }
    } else if sc < 0x80 {
        sc as u8
    } else {
        0
    }
}

/// `schar_get(char *buf_out, schar_T sc)` - the glyph's own bytes.
///
/// Returns them owned rather than filling a caller-provided buffer:
/// the original's `buf_out`/`schar_get_adv` split exists purely to let
/// a caller append into a larger scratch buffer without a second copy,
/// which is not a distinction Rust needs at this call depth. The
/// original also writes a trailing NUL, which is part of its C string
/// representation rather than part of the glyph.
#[must_use]
pub fn schar_get(sc: ScharT) -> Vec<u8> {
    if schar_high(sc) {
        // SAFETY: a plain global-cell borrow, no aliasing hazard.
        let cache = unsafe { GLYPH_CACHE.get_mut() };
        let idx = schar_idx(sc) as usize;
        debug_assert!(idx < cache.len());
        cache.keys().get(idx).cloned().unwrap_or_default()
    } else {
        let bytes = sc.to_ne_bytes();
        let len = bytes.iter().position(|&b| b == 0).unwrap_or(4);
        bytes[..len].to_vec()
    }
}

/// The first raw UTF-8 byte of `sc` (`schar_get_first_byte`).
fn schar_get_first_byte(sc: ScharT) -> u8 {
    if schar_high(sc) {
        // SAFETY: a plain global-cell borrow, no aliasing hazard.
        let cache = unsafe { GLYPH_CACHE.get_mut() };
        let idx = schar_idx(sc) as usize;
        debug_assert!(idx < cache.len());
        cache.keys().get(idx).and_then(|k| k.first()).copied().unwrap_or(0)
    } else {
        sc.to_ne_bytes()[0]
    }
}

/// Whether `sc` starts in the Arabic Unicode block
/// (`schar_in_arabic_block`).
///
/// Tests only the FIRST UTF-8 byte: `0xD8`/`0xD9` are the two lead
/// bytes covering U+0600..U+07FF, so masking off the low bit of the
/// lead byte identifies both at once. This is a cheap pre-filter, not
/// an exact test - `line_do_arabic_shape` re-checks with
/// `ARABIC_CHAR` before shaping anything.
fn schar_in_arabic_block(sc: ScharT) -> bool {
    (schar_get_first_byte(sc) & 0xFE) == 0xD8
}

/// `ARABIC_CHAR(ch)` (`arabic.h`) - whether `ch` is in U+0600..U+06FF.
const fn arabic_char(ch: i32) -> bool {
    (ch & 0xFF00) == 0x0600
}

/// The first two codepoints of `sc`, or `0` when not available
/// (`schar_get_first_two_codepoints`).
fn schar_get_first_two_codepoints(sc: ScharT) -> (i32, i32) {
    let buf = schar_get_nul_terminated(sc);
    let c0 = crate::mbyte::utf_ptr2char(&buf);
    if c0 == 0 {
        return (0, 0);
    }
    let len = usize::try_from(crate::mbyte::utf_ptr2len(&buf)).unwrap_or(0);
    let c1 = if len < buf.len() {
        crate::mbyte::utf_ptr2char(&buf[len..])
    } else {
        0
    };
    (c0, c1)
}

/// Apply Arabic shaping to a whole line of glyphs in place
/// (`line_do_arabic_shape`).
///
/// The original takes a `schar_T *buf` plus a separate `cols` count; a
/// slice carries both, so a caller wanting to shape only part of a
/// line passes `&mut buf[..cols]`.
///
/// Note the argument order at the `arabic_shape` call: the NEXT
/// character is passed as that function's `prev_c`/`prev_c1`, and the
/// PREVIOUS one as its `next_c`. That is not a mistake - the glyph
/// buffer is in VISUAL order while `arabic_shape` reasons in LOGICAL
/// order, and Arabic runs right-to-left, so the two are mirrored.
/// Preserved exactly as the original has it.
///
/// # Safety
/// Forwarded from [`crate::arabic::arabic_shape`]'s own safety doc.
pub unsafe fn line_do_arabic_shape(buf: &mut [ScharT]) {
    let cols = buf.len();

    // quickly skip over non-arabic text
    let Some(start) = buf.iter().position(|&sc| schar_in_arabic_block(sc)) else {
        return;
    };

    let mut c0prev = 0i32;
    let (mut c0, mut c1) = schar_get_first_two_codepoints(buf[start]);

    for i in start..cols {
        let (c0next, c1next) = schar_get_first_two_codepoints(if i + 1 < cols {
            buf[i + 1]
        } else {
            0
        });

        if arabic_char(c0) {
            // SAFETY: forwarded from this function's own safety doc.
            let (c0new, c1new) = unsafe {
                // Visual order vs logical order - see this function's
                // own doc comment.
                crate::arabic::arabic_shape(c0, c1, c0next, c1next, c0prev)
            };

            if c0new != c0 || c1new != c1 {
                let sc_bytes = schar_get(buf[i]);
                let mut scbuf_new = [0u8; MAX_SCHAR_SIZE];
                let mut len = usize::try_from(crate::mbyte::utf_char2bytes(c0new, &mut scbuf_new))
                    .unwrap_or(0);
                if c1new != 0 {
                    len += usize::try_from(crate::mbyte::utf_char2bytes(
                        c1new,
                        &mut scbuf_new[len..],
                    ))
                    .unwrap_or(0);
                }

                let off = usize::try_from(
                    crate::mbyte::utf_char2len(c0)
                        + if c1 != 0 {
                            crate::mbyte::utf_char2len(c1)
                        } else {
                            0
                        },
                )
                .unwrap_or(0);
                let mut rest = sc_bytes.len().saturating_sub(off);

                if rest > 0 && rest + len + 1 > MAX_SCHAR_SIZE {
                    // Too bigly, discard one code-point. This is
                    // enough because c0 cannot grow by more than two
                    // bytes (base arabic to extended arabic).
                    let tail = &sc_bytes[off..off + rest];
                    let b = crate::mbyte::utf_cp_bounds(tail, rest - 1);
                    rest -= usize::try_from(b.begin_off).unwrap_or(0) + 1;
                }

                scbuf_new[len..len + rest].copy_from_slice(&sc_bytes[off..off + rest]);
                buf[i] = schar_from_buf(&scbuf_new[..len + rest]);
            }
        }

        c0prev = c0;
        c0 = c0next;
        c1 = c1next;
    }
}

/// Clear a line in the grid starting at `off` until `width`
/// characters are cleared (`grid_clear_line`).
///
/// `valid` false marks the cleared attributes invalid (`-1`), which is
/// how the redraw code flags a row it must repaint rather than trust.
///
/// # Safety
/// `grid`'s own `chars`/`attrs`/`vcols` must each point to at least
/// `off + width` live elements.
pub unsafe fn grid_clear_line(
    grid: &mut crate::grid_defs::ScreenGrid,
    off: usize,
    width: i32,
    valid: bool,
) {
    let width = usize::try_from(width).unwrap_or(0);
    let fill: crate::types_defs::SattrT = if valid { 0 } else { -1 };
    for col in 0..width {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            *grid.chars.add(off + col) = schar_from_ascii(b' ');
            *grid.attrs.add(off + col) = fill;
            // The original `memset`s -1 across every byte, which for
            // a two's-complement `colnr_T` is exactly -1.
            *grid.vcols.add(off + col) = -1;
        }
    }
}

/// Mark every attribute in `grid` invalid, forcing a full repaint
/// (`grid_invalidate`).
///
/// # Safety
/// `grid.attrs` must point to at least `rows * cols` live elements.
pub unsafe fn grid_invalidate(grid: &mut crate::grid_defs::ScreenGrid) {
    let n = usize::try_from(grid.rows).unwrap_or(0) * usize::try_from(grid.cols).unwrap_or(0);
    for i in 0..n {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            *grid.attrs.add(i) = -1;
        }
    }
}

/// Get a single character directly from `grid.chars`
/// (`grid_getchar`).
///
/// Returns `(schar, attr)`. The original takes `attrp` as an optional
/// out-parameter; a tuple says the same thing, and a caller that does
/// not want the attribute simply ignores it.
///
/// Returns `(0, 0)` when the position is out of range, matching the
/// original's own safety check.
///
/// # Safety
/// When `grid.chars` is non-null, `grid`'s own `chars`/`attrs`/
/// `line_offset` must point to live elements covering `row`/`col`.
#[must_use]
pub unsafe fn grid_getchar(
    grid: &crate::grid_defs::ScreenGrid,
    row: i32,
    col: i32,
) -> (ScharT, i32) {
    // safety check
    if grid.chars.is_null() || row >= grid.rows || col >= grid.cols {
        return (0, 0);
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        let off = *grid.line_offset.add(row as usize) + col as usize;
        (*grid.chars.add(off), *grid.attrs.add(off))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct GridLineStateGuard([i32; 9]);

    impl GridLineStateGuard {
        fn install(first: i32, last: i32) -> Self {
            let saved = unsafe {
                [
                    *GRID_LINE_FIRST.get_mut(),
                    *GRID_LINE_LAST.get_mut(),
                    *GRID_LINE_CLEAR_TO.get_mut(),
                    *GRID_LINE_BG_ATTR.get_mut(),
                    *GRID_LINE_CLEAR_ATTR.get_mut(),
                    *GRID_LINE_FLAGS.get_mut(),
                    *GRID_LINE_MAXCOL.get_mut(),
                    *GRID_LINE_ROW.get_mut(),
                    *GRID_LINE_COLOFF.get_mut(),
                ]
            };
            unsafe {
                *GRID_LINE_FIRST.get_mut() = first;
                *GRID_LINE_LAST.get_mut() = last;
                *GRID_LINE_FLAGS.get_mut() = 0;
                *GRID_LINE_MAXCOL.get_mut() = 0;
                *GRID_LINE_ROW.get_mut() = -1;
                *GRID_LINE_COLOFF.get_mut() = 0;
            }
            Self(saved)
        }
    }

    impl Drop for GridLineStateGuard {
        fn drop(&mut self) {
            unsafe {
                *GRID_LINE_FIRST.get_mut() = self.0[0];
                *GRID_LINE_LAST.get_mut() = self.0[1];
                *GRID_LINE_CLEAR_TO.get_mut() = self.0[2];
                *GRID_LINE_BG_ATTR.get_mut() = self.0[3];
                *GRID_LINE_CLEAR_ATTR.get_mut() = self.0[4];
                *GRID_LINE_FLAGS.get_mut() = self.0[5];
                *GRID_LINE_MAXCOL.get_mut() = self.0[6];
                *GRID_LINE_ROW.get_mut() = self.0[7];
                *GRID_LINE_COLOFF.get_mut() = self.0[8];
            }
        }
    }

    struct LinebufStateGuard {
        chars: Vec<ScharT>,
        attrs: Vec<crate::types_defs::SattrT>,
        vcols: Vec<crate::pos_defs::ColnrT>,
    }

    impl LinebufStateGuard {
        fn install(
            chars: Vec<ScharT>,
            attrs: Vec<crate::types_defs::SattrT>,
            vcols: Vec<crate::pos_defs::ColnrT>,
        ) -> Self {
            unsafe {
                Self {
                    chars: std::mem::replace(LINEBUF_CHAR.get_mut(), chars),
                    attrs: std::mem::replace(LINEBUF_ATTR.get_mut(), attrs),
                    vcols: std::mem::replace(LINEBUF_VCOL.get_mut(), vcols),
                }
            }
        }
    }

    impl Drop for LinebufStateGuard {
        fn drop(&mut self) {
            unsafe {
                *LINEBUF_CHAR.get_mut() = std::mem::take(&mut self.chars);
                *LINEBUF_ATTR.get_mut() = std::mem::take(&mut self.attrs);
                *LINEBUF_VCOL.get_mut() = std::mem::take(&mut self.vcols);
            }
        }
    }

    struct GridLineGridGuard(*mut crate::grid_defs::ScreenGrid);

    impl GridLineGridGuard {
        fn install(grid: *mut crate::grid_defs::ScreenGrid) -> Self {
            let saved = unsafe { *GRID_LINE_GRID.get_mut() };
            unsafe { *GRID_LINE_GRID.get_mut() = grid };
            Self(saved)
        }
    }

    struct RdbFlagsGuard(u32);

    impl RdbFlagsGuard {
        fn install(value: u32) -> Self {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let saved = opts.rdb_flags;
            opts.rdb_flags = value;
            Self(saved)
        }
    }

    impl Drop for RdbFlagsGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.rdb_flags = self.0;
        }
    }

    #[test]
    fn screengrid_line_start_initializes_all_buffering_state() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = GridLineStateGuard::install(1, 2);
        let _buf = LinebufStateGuard::install(
            vec![0; 6],
            vec![0; 6],
            vec![0; 6],
        );
        let _active = GridLineGridGuard::install(std::ptr::null_mut());
        let mut grid = crate::grid_defs::ScreenGrid {
            cols: 6,
            ..Default::default()
        };
        let grid_ptr = std::ptr::addr_of_mut!(grid);

        unsafe { screengrid_line_start(grid_ptr, 3, 2) };

        assert_eq!(unsafe { *GRID_LINE_GRID.get_mut() }, grid_ptr);
        assert_eq!(unsafe { *GRID_LINE_ROW.get_mut() }, 3);
        assert_eq!(unsafe { *GRID_LINE_COLOFF.get_mut() }, 2);
        assert_eq!(unsafe { *GRID_LINE_MAXCOL.get_mut() }, 4);
        assert_eq!(unsafe { *GRID_LINE_FIRST.get_mut() }, 6);
        assert_eq!(unsafe { *GRID_LINE_LAST.get_mut() }, 0);
        assert_eq!(unsafe { *GRID_LINE_FLAGS.get_mut() }, 0);
    }

    #[test]
    fn screengrid_line_start_invalidates_scratch_cells_when_requested() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = GridLineStateGuard::install(1, 2);
        let _buf = LinebufStateGuard::install(
            vec![1; 4],
            vec![2; 4],
            vec![3; 4],
        );
        let _active = GridLineGridGuard::install(std::ptr::null_mut());
        let _full = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.full_screen,
                true,
            )
        };
        let _rdb = RdbFlagsGuard::install(
            crate::option_vars::opt_rdb_flag::INVALID,
        );
        let mut grid = crate::grid_defs::ScreenGrid {
            cols: 4,
            ..Default::default()
        };

        unsafe {
            screengrid_line_start(std::ptr::addr_of_mut!(grid), 0, 0);
        }

        assert_eq!(unsafe { &*LINEBUF_CHAR.get_mut() }, &[ScharT::MAX; 4]);
        assert_eq!(unsafe { &*LINEBUF_ATTR.get_mut() }, &[-1; 4]);
        assert_eq!(unsafe { &*LINEBUF_VCOL.get_mut() }, &[3; 4]);
    }

    #[test]
    fn grid_line_start_applies_the_grid_views_offsets() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = GridLineStateGuard::install(1, 2);
        let _buf = LinebufStateGuard::install(
            vec![0; 10],
            vec![0; 10],
            vec![0; 10],
        );
        let _active = GridLineGridGuard::install(std::ptr::null_mut());
        let mut grid = crate::grid_defs::ScreenGrid {
            cols: 10,
            ..Default::default()
        };
        let grid_ptr = std::ptr::addr_of_mut!(grid);
        let view = crate::grid_defs::GridView {
            target: grid_ptr,
            row_offset: 2,
            col_offset: 3,
        };

        unsafe { grid_line_start(&view, 4) };

        assert_eq!(unsafe { *GRID_LINE_GRID.get_mut() }, grid_ptr);
        assert_eq!(unsafe { *GRID_LINE_ROW.get_mut() }, 6);
        assert_eq!(unsafe { *GRID_LINE_COLOFF.get_mut() }, 3);
        assert_eq!(unsafe { *GRID_LINE_MAXCOL.get_mut() }, 7);
    }

    #[test]
    fn grid_line_getchar_reads_the_active_grid_with_column_offset() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = GridLineStateGuard::install(1, 2);
        let _buf = LinebufStateGuard::install(
            vec![0; 5],
            vec![0; 5],
            vec![0; 5],
        );
        let _active = GridLineGridGuard::install(std::ptr::null_mut());
        let mut chars = vec![0; 10];
        let mut attrs = vec![0; 10];
        let mut offsets = [0_usize, 5];
        chars[8] = 99;
        attrs[8] = 7;
        let mut grid = crate::grid_defs::ScreenGrid {
            chars: chars.as_mut_ptr(),
            attrs: attrs.as_mut_ptr(),
            line_offset: offsets.as_mut_ptr(),
            rows: 2,
            cols: 5,
            ..Default::default()
        };
        unsafe {
            screengrid_line_start(std::ptr::addr_of_mut!(grid), 1, 2);
        }

        assert_eq!(unsafe { grid_line_getchar(1) }, (99, Some(7)));
    }

    #[test]
    fn grid_line_getchar_returns_neutral_space_past_maxcol() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = GridLineStateGuard::install(1, 2);
        let _buf = LinebufStateGuard::install(
            vec![0; 3],
            vec![0; 3],
            vec![0; 3],
        );
        let _active = GridLineGridGuard::install(std::ptr::null_mut());
        let mut grid = crate::grid_defs::ScreenGrid {
            cols: 3,
            ..Default::default()
        };
        unsafe {
            screengrid_line_start(std::ptr::addr_of_mut!(grid), 0, 0);
        }

        assert_eq!(
            unsafe { grid_line_getchar(3) },
            (schar_from_ascii(b' '), None)
        );
    }

    impl Drop for GridLineGridGuard {
        fn drop(&mut self) {
            unsafe { *GRID_LINE_GRID.get_mut() = self.0 };
        }
    }

    #[test]
    fn grid_line_clear_end_starts_a_new_range_before_existing_output() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GridLineStateGuard::install(20, 30);

        grid_line_clear_end(10, 40, 7, 9);

        assert_eq!(unsafe { *GRID_LINE_FIRST.get_mut() }, 10);
        assert_eq!(unsafe { *GRID_LINE_LAST.get_mut() }, 10);
        assert_eq!(unsafe { *GRID_LINE_CLEAR_TO.get_mut() }, 40);
        assert_eq!(unsafe { *GRID_LINE_BG_ATTR.get_mut() }, 7);
        assert_eq!(unsafe { *GRID_LINE_CLEAR_ATTR.get_mut() }, 9);
    }

    #[test]
    fn grid_line_clear_end_keeps_an_earlier_buffered_range() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GridLineStateGuard::install(5, 12);

        grid_line_clear_end(10, 40, 7, 9);

        assert_eq!(unsafe { *GRID_LINE_FIRST.get_mut() }, 5);
        assert_eq!(unsafe { *GRID_LINE_LAST.get_mut() }, 12);
        assert_eq!(unsafe { *GRID_LINE_CLEAR_TO.get_mut() }, 40);
    }

    #[test]
    fn grid_line_fill_clamps_fills_and_updates_the_dirty_range() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = GridLineStateGuard::install(8, 1);
        unsafe { *GRID_LINE_MAXCOL.get_mut() = 5 };
        let _buf = LinebufStateGuard::install(
            vec![0; 6],
            vec![0; 6],
            vec![7; 6],
        );

        let end = unsafe { grid_line_fill(2, 99, 42, 11) };

        assert_eq!(end, 5);
        assert_eq!(unsafe { &*LINEBUF_CHAR.get_mut() }, &[0, 0, 42, 42, 42, 0]);
        assert_eq!(unsafe { &*LINEBUF_ATTR.get_mut() }, &[0, 0, 11, 11, 11, 0]);
        assert_eq!(unsafe { &*LINEBUF_VCOL.get_mut() }, &[7, 7, -1, -1, -1, 7]);
        assert_eq!(unsafe { *GRID_LINE_FIRST.get_mut() }, 2);
        assert_eq!(unsafe { *GRID_LINE_LAST.get_mut() }, 5);
    }

    #[test]
    fn grid_line_fill_is_a_noop_for_an_empty_clamped_range() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = GridLineStateGuard::install(1, 3);
        unsafe { *GRID_LINE_MAXCOL.get_mut() = 4 };
        let _buf = LinebufStateGuard::install(
            vec![1; 4],
            vec![2; 4],
            vec![3; 4],
        );

        assert_eq!(unsafe { grid_line_fill(4, 9, 8, 9) }, 4);
        assert_eq!(unsafe { &*LINEBUF_CHAR.get_mut() }, &[1; 4]);
        assert_eq!(unsafe { *GRID_LINE_FIRST.get_mut() }, 1);
        assert_eq!(unsafe { *GRID_LINE_LAST.get_mut() }, 3);
    }

    #[test]
    fn grid_line_put_schar_updates_one_cell_and_the_dirty_range() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = GridLineStateGuard::install(8, 1);
        unsafe { *GRID_LINE_MAXCOL.get_mut() = 5 };
        let _buf = LinebufStateGuard::install(
            vec![0; 5],
            vec![0; 5],
            vec![7; 5],
        );
        let mut grid = crate::grid_defs::ScreenGrid::default();
        let _grid = GridLineGridGuard::install(std::ptr::addr_of_mut!(grid));

        unsafe { grid_line_put_schar(2, 42, 9) };

        assert_eq!(unsafe { (&*LINEBUF_CHAR.get_mut())[2] }, 42);
        assert_eq!(unsafe { (&*LINEBUF_ATTR.get_mut())[2] }, 9);
        assert_eq!(unsafe { (&*LINEBUF_VCOL.get_mut())[2] }, -1);
        assert_eq!(unsafe { *GRID_LINE_FIRST.get_mut() }, 2);
        assert_eq!(unsafe { *GRID_LINE_LAST.get_mut() }, 3);
    }

    #[test]
    fn grid_line_put_schar_ignores_columns_at_or_past_maxcol() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = GridLineStateGuard::install(1, 2);
        unsafe { *GRID_LINE_MAXCOL.get_mut() = 3 };
        let _buf = LinebufStateGuard::install(
            vec![0; 3],
            vec![0; 3],
            vec![0; 3],
        );
        let mut grid = crate::grid_defs::ScreenGrid::default();
        let _grid = GridLineGridGuard::install(std::ptr::addr_of_mut!(grid));

        unsafe { grid_line_put_schar(3, 42, 9) };

        assert_eq!(unsafe { &*LINEBUF_CHAR.get_mut() }, &[0, 0, 0]);
        assert_eq!(unsafe { *GRID_LINE_FIRST.get_mut() }, 1);
        assert_eq!(unsafe { *GRID_LINE_LAST.get_mut() }, 2);
    }

    #[test]
    fn linebuf_mirror_reverses_characters_attributes_and_virtual_columns() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = LinebufStateGuard::install(
            vec![b'A' as u32, b'B' as u32, b'C' as u32, b'D' as u32],
            vec![1, 2, 3, 4],
            vec![10, 20, 30, 40],
        );
        let (mut first, mut last, mut clear) = (0, 4, 4);

        unsafe { linebuf_mirror(&mut first, &mut last, &mut clear, 4) };

        assert_eq!(unsafe { &*LINEBUF_CHAR.get_mut() }, &[b'D' as u32, b'C' as u32, b'B' as u32, b'A' as u32]);
        assert_eq!(unsafe { &*LINEBUF_ATTR.get_mut() }, &[4, 3, 2, 1]);
        assert_eq!(unsafe { &*LINEBUF_VCOL.get_mut() }, &[40, 30, 20, 10]);
        assert_eq!((first, last, clear), (0, 0, 4));
    }

    #[test]
    fn linebuf_mirror_keeps_a_double_width_glyph_before_its_zero_cell() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = LinebufStateGuard::install(
            vec![0x4e00, 0, b'A' as u32],
            vec![7, 7, 8],
            vec![1, 1, 2],
        );
        let (mut first, mut last, mut clear) = (0, 3, 3);

        unsafe { linebuf_mirror(&mut first, &mut last, &mut clear, 3) };

        assert_eq!(unsafe { &*LINEBUF_CHAR.get_mut() }, &[b'A' as u32, 0x4e00, 0]);
    }

    #[test]
    fn grid_line_mirror_extends_the_clear_range_and_sets_rightleft() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = GridLineStateGuard::install(0, 4);
        unsafe { *GRID_LINE_CLEAR_TO.get_mut() = 2 };
        let _buf = LinebufStateGuard::install(
            vec![1, 2, 3, 4],
            vec![1, 2, 3, 4],
            vec![1, 2, 3, 4],
        );

        unsafe { grid_line_mirror(4) };

        assert_eq!(unsafe { &*LINEBUF_CHAR.get_mut() }, &[4, 3, 2, 1]);
        assert_eq!(unsafe { *GRID_LINE_CLEAR_TO.get_mut() }, 4);
        assert_ne!(unsafe { *GRID_LINE_FLAGS.get_mut() } & SLF_RIGHTLEFT, 0);
    }

    #[test]
    fn grid_line_mirror_returns_without_touching_an_empty_range() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = GridLineStateGuard::install(4, 4);
        unsafe { *GRID_LINE_CLEAR_TO.get_mut() = 4 };
        let _buf = LinebufStateGuard::install(
            vec![1, 2, 3, 4],
            vec![1, 2, 3, 4],
            vec![1, 2, 3, 4],
        );

        unsafe { grid_line_mirror(4) };

        assert_eq!(unsafe { &*LINEBUF_CHAR.get_mut() }, &[1, 2, 3, 4]);
        assert_eq!(unsafe { *GRID_LINE_FLAGS.get_mut() }, 0);
    }

    // --- grid_char_needs_redraw ---

    /// A grid backed by real arrays, plus the line buffers set to the
    /// values being compared against it.
    fn redraw_fixture(
        grid_chars: &mut [ScharT],
        grid_attrs: &mut [crate::types_defs::SattrT],
        buf_chars: &[ScharT],
        buf_attrs: &[crate::types_defs::SattrT],
    ) -> crate::grid_defs::ScreenGrid {
        unsafe {
            *LINEBUF_CHAR.get_mut() = buf_chars.to_vec();
            *LINEBUF_ATTR.get_mut() = buf_attrs.to_vec();
        }
        crate::grid_defs::ScreenGrid {
            chars: grid_chars.as_mut_ptr(),
            attrs: grid_attrs.as_mut_ptr(),
            rows: 1,
            cols: grid_chars.len() as i32,
            ..Default::default()
        }
    }

    fn clear_linebufs() {
        unsafe {
            LINEBUF_CHAR.get_mut().clear();
            LINEBUF_ATTR.get_mut().clear();
        }
    }

    #[test]
    fn grid_char_needs_redraw_is_false_when_nothing_changed() {
        let _lock = crate::globals::global_state_test_lock();
        let mut gc = [65u32, 66];
        let mut ga = [1i32, 1];
        let grid = redraw_fixture(&mut gc, &mut ga, &[65, 66], &[1, 1]);

        let got = unsafe { grid_char_needs_redraw(&grid, 0, 0, 1) };
        clear_linebufs();
        assert!(!got);
    }

    #[test]
    fn grid_char_needs_redraw_detects_a_changed_character_or_attribute() {
        let _lock = crate::globals::global_state_test_lock();

        let mut gc = [65u32, 66];
        let mut ga = [1i32, 1];
        let grid = redraw_fixture(&mut gc, &mut ga, &[99, 66], &[1, 1]);
        let char_changed = unsafe { grid_char_needs_redraw(&grid, 0, 0, 1) };

        let mut gc2 = [65u32, 66];
        let mut ga2 = [1i32, 1];
        let grid2 = redraw_fixture(&mut gc2, &mut ga2, &[65, 66], &[9, 1]);
        let attr_changed = unsafe { grid_char_needs_redraw(&grid2, 0, 0, 1) };

        clear_linebufs();
        assert!(char_changed, "a different character needs a redraw");
        assert!(attr_changed, "so does a different attribute");
    }

    #[test]
    fn grid_char_needs_redraw_checks_the_second_cell_of_a_wide_character() {
        // A double-width character stores 0 in its second cell. If the
        // grid disagrees there, the pair needs redrawing even though
        // the first cell and its attribute both match.
        let _lock = crate::globals::global_state_test_lock();
        let mut gc = [65u32, 77];
        let mut ga = [1i32, 1];
        let grid = redraw_fixture(&mut gc, &mut ga, &[65, 0], &[1, 1]);

        let needs = unsafe { grid_char_needs_redraw(&grid, 0, 0, 2) };
        // With cols == 1 the second cell is not consulted at all.
        let ignored = unsafe { grid_char_needs_redraw(&grid, 0, 0, 1) };

        clear_linebufs();
        assert!(needs);
        assert!(!ignored, "the second cell only matters when cols > 1");
    }

    #[test]
    fn grid_char_needs_redraw_is_false_for_a_zero_width_run() {
        let _lock = crate::globals::global_state_test_lock();
        let mut gc = [99u32];
        let mut ga = [9i32];
        let grid = redraw_fixture(&mut gc, &mut ga, &[65], &[1]);

        let got = unsafe { grid_char_needs_redraw(&grid, 0, 0, 0) };
        clear_linebufs();
        assert!(!got, "nothing to draw, however different it is");
    }

    #[test]
    fn grid_char_needs_redraw_is_forced_by_nodelta() {
        // 'redrawdebug' nodelta forces every cell to be redrawn even
        // when it is identical.
        let _lock = crate::globals::global_state_test_lock();
        let mut gc = [65u32];
        let mut ga = [1i32];
        let grid = redraw_fixture(&mut gc, &mut ga, &[65], &[1]);

        let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = ov.rdb_flags;
        ov.rdb_flags = crate::option_vars::opt_rdb_flag::NODELTA;

        let got = unsafe { grid_char_needs_redraw(&grid, 0, 0, 1) };

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.rdb_flags = prev;
        clear_linebufs();
        assert!(got);
    }

    // --- get_bordertext_col ---

    #[test]
    fn get_bordertext_col_left_always_starts_at_column_one() {
        use crate::buffer_defs::AlignTextPos;
        assert_eq!(get_bordertext_col(20, 5, AlignTextPos::Left), 1);
        // Even when the text is wider than the border.
        assert_eq!(get_bordertext_col(4, 30, AlignTextPos::Left), 1);
    }

    #[test]
    fn get_bordertext_col_centers_and_right_aligns() {
        use crate::buffer_defs::AlignTextPos;
        // 20 wide, 6 of text: (20-6)/2 + 1 = 8.
        assert_eq!(get_bordertext_col(20, 6, AlignTextPos::Center), 8);
        // Right: 20 - 6 + 1 = 15.
        assert_eq!(get_bordertext_col(20, 6, AlignTextPos::Right), 15);
    }

    #[test]
    fn get_bordertext_col_never_returns_less_than_one() {
        // Text wider than the border would compute a column below 1,
        // which would run off the left edge; both alignments clamp.
        use crate::buffer_defs::AlignTextPos;
        assert_eq!(get_bordertext_col(4, 30, AlignTextPos::Center), 1);
        assert_eq!(get_bordertext_col(4, 30, AlignTextPos::Right), 1);
    }

    // --- grid_adjust / grid_invalid_row ---

    #[test]
    fn grid_adjust_adds_the_views_own_offsets() {
        let mut target = crate::grid_defs::ScreenGrid::default();
        let target_ptr = std::ptr::addr_of_mut!(target);
        let view = crate::grid_defs::GridView {
            target: target_ptr,
            row_offset: 3,
            col_offset: 10,
        };

        let (g, row, col) = grid_adjust(&view, 5, 2);
        assert_eq!(g, target_ptr);
        assert_eq!(row, 8);
        assert_eq!(col, 12);
    }

    #[test]
    fn grid_adjust_with_no_offsets_passes_positions_through() {
        let mut target = crate::grid_defs::ScreenGrid::default();
        let target_ptr = std::ptr::addr_of_mut!(target);
        let view = crate::grid_defs::GridView { target: target_ptr, ..Default::default() };

        let (_g, row, col) = grid_adjust(&view, 4, 7);
        assert_eq!((row, col), (4, 7));
    }

    #[test]
    fn grid_invalid_row_reads_the_first_cell_of_that_row() {
        // A negative attribute in a row's first cell is the marker
        // grid_invalidate leaves behind; row 1 is invalid here and
        // row 0 is not, so a version ignoring line_offset would fail.
        let mut attrs: Vec<crate::types_defs::SattrT> = vec![0, 0, -1, 0];
        let mut offsets: Vec<usize> = vec![0, 2];
        let grid = crate::grid_defs::ScreenGrid {
            attrs: attrs.as_mut_ptr(),
            line_offset: offsets.as_mut_ptr(),
            rows: 2,
            cols: 2,
            ..Default::default()
        };

        assert!(!unsafe { grid_invalid_row(&grid, 0) });
        assert!(unsafe { grid_invalid_row(&grid, 1) });
    }

    #[test]
    fn grid_assign_handle_gives_each_grid_a_distinct_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut a = crate::grid_defs::ScreenGrid::default();
        let mut b = crate::grid_defs::ScreenGrid::default();

        unsafe { grid_assign_handle(&mut a) };
        unsafe { grid_assign_handle(&mut b) };

        assert_ne!(a.handle, 0);
        assert_ne!(b.handle, 0);
        assert_ne!(a.handle, b.handle, "handles must be unique");
    }

    #[test]
    fn grid_assign_handle_leaves_an_existing_handle_alone() {
        // A grid keeps its identity across reallocation, so a handle
        // that is already set must not be reassigned.
        let _lock = crate::globals::global_state_test_lock();
        let mut g = crate::grid_defs::ScreenGrid { handle: 42, ..Default::default() };

        unsafe { grid_assign_handle(&mut g) };

        assert_eq!(g.handle, 42);
    }

    #[test]
    fn get_win_by_grid_handle_finds_the_matching_window() {
        // Boxed: these pointers are installed into GLOBALS.
        let _lock = crate::globals::global_state_test_lock();
        let mut w1 = Box::new(crate::buffer_defs::WinT::default());
        let mut w2 = Box::new(crate::buffer_defs::WinT::default());
        w1.w_grid_alloc.handle = 7;
        w2.w_grid_alloc.handle = 9;
        let w1_ptr = std::ptr::addr_of_mut!(*w1);
        let w2_ptr = std::ptr::addr_of_mut!(*w2);
        w1.w_next = w2_ptr;
        w2.w_next = std::ptr::null_mut();

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.firstwin;
        g.firstwin = w1_ptr;

        let found_first = unsafe { get_win_by_grid_handle(7) };
        let found_second = unsafe { get_win_by_grid_handle(9) };
        let missing = unsafe { get_win_by_grid_handle(1234) };

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev;

        assert_eq!(found_first, w1_ptr);
        assert_eq!(found_second, w2_ptr, "the walk continues past the first window");
        assert!(missing.is_null(), "an unknown handle finds nothing");
    }

    #[test]
    fn get_win_by_grid_handle_is_null_with_no_windows() {
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.firstwin;
        g.firstwin = std::ptr::null_mut();

        let found = unsafe { get_win_by_grid_handle(1) };

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev;
        assert!(found.is_null());
    }

    /// The glyph cache is process-wide, so any test touching it must
    /// hold the same lock every other global-state test holds.
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        crate::globals::global_state_test_lock()
    }

    #[test]
    fn schar_from_ascii_round_trips_through_schar_get() {
        let _l = lock();
        for c in *b"a ~@" {
            let sc = schar_from_ascii(c);
            assert!(!schar_high(sc));
            assert_eq!(schar_get(sc), vec![c]);
        }
    }

    #[test]
    fn schar_from_buf_stores_short_glyphs_inline() {
        let _l = lock();
        // One, two and three byte UTF-8 all fit in the four inline
        // bytes, so none of them touch the cache.
        for s in ["x", "é", "─", "│", "abcd"] {
            let sc = schar_from_buf(s.as_bytes());
            assert!(!schar_high(sc), "{s} should be inline");
            assert_eq!(schar_get(sc), s.as_bytes());
        }
    }

    #[test]
    fn schar_from_buf_interns_long_glyphs_and_reuses_the_entry() {
        let _l = lock();
        // Five bytes is one past the inline limit.
        let long = "a\u{0301}\u{0302}".as_bytes();
        assert!(long.len() > 4);
        let sc = schar_from_buf(long);
        assert!(schar_high(sc));
        assert_eq!(schar_get(sc), long);
        // Interning the same glyph again yields the same value.
        assert_eq!(schar_from_buf(long), sc);
    }

    #[test]
    fn schar_from_buf_gives_distinct_values_to_distinct_long_glyphs() {
        let _l = lock();
        let a = "a\u{0301}\u{0302}".as_bytes();
        let b = "b\u{0301}\u{0302}".as_bytes();
        let sa = schar_from_buf(a);
        let sb = schar_from_buf(b);
        assert_ne!(sa, sb);
        assert_eq!(schar_get(sa), a);
        assert_eq!(schar_get(sb), b);
    }

    #[test]
    fn schar_from_char_encodes_a_codepoint_inline() {
        let _l = lock();
        for (c, s) in [(0x61, "a"), (0xE9, "é"), (0x2500, "─"), (0x1F600, "\u{1F600}")] {
            assert_eq!(schar_get(schar_from_char(c)), s.as_bytes(), "U+{c:04X}");
        }
    }

    #[test]
    fn schar_from_char_clamps_an_out_of_range_codepoint_to_replacement() {
        let _l = lock();
        // The original clamps anything >= 0x200000 to U+FFFD.
        assert_eq!(
            schar_from_char(0x0020_0000),
            schar_from_char(0xFFFD),
            "at the limit"
        );
        assert_eq!(schar_get(schar_from_char(0x0030_0000)), "\u{FFFD}".as_bytes());
        // One below the limit still encodes as itself.
        assert_ne!(schar_from_char(0x001F_FFFF), schar_from_char(0xFFFD));
    }

    #[test]
    fn schar_len_and_cells_agree_for_inline_and_interned_glyphs() {
        let _l = lock();
        // (glyph, expected byte length, expected screen cells)
        for (s, len, cells) in [
            ("a", 1usize, 1i32),
            ("é", 2, 1),
            ("─", 3, 1),
            ("一", 3, 2),
            ("a\u{0301}\u{0302}", 5, 1),
        ] {
            let sc = schar_from_buf(s.as_bytes());
            assert_eq!(schar_len(sc), len, "len of {s}");
            assert_eq!(schar_cells(sc), cells, "cells of {s}");
        }
    }

    #[test]
    fn schar_cells_takes_the_ascii_fast_path() {
        let _l = lock();
        for c in *b"a ~@" {
            assert_eq!(schar_cells(schar_from_ascii(c)), 1);
        }
    }

    #[test]
    fn schar_get_ascii_returns_the_byte_only_for_ascii() {
        let _l = lock();
        for c in *b"a ~@" {
            assert_eq!(schar_get_ascii(schar_from_ascii(c)), c);
        }
        // A non-ASCII glyph yields NUL instead.
        assert_eq!(schar_get_ascii(schar_from_buf("é".as_bytes())), 0);
        assert_eq!(schar_get_ascii(schar_from_buf("─".as_bytes())), 0);
    }

    #[test]
    fn schar_get_first_codepoint_reads_through_the_cache() {
        let _l = lock();
        assert_eq!(schar_get_first_codepoint(schar_from_ascii(b'a')), 0x61);
        assert_eq!(schar_get_first_codepoint(schar_from_buf("─".as_bytes())), 0x2500);
        // An interned (>4 byte) glyph reports its BASE codepoint.
        let long = schar_from_buf("a\u{0301}\u{0302}".as_bytes());
        assert!(schar_high(long));
        assert_eq!(schar_get_first_codepoint(long), 0x61);
    }

    #[test]
    fn schar_accessors_handle_the_empty_glyph() {
        let _l = lock();
        // Zero decodes to no bytes at all. The NUL terminator that
        // `schar_get_nul_terminated` re-adds is what keeps
        // `utf_ptr2char` from indexing an empty slice here.
        assert_eq!(schar_len(0), 0);
        assert_eq!(schar_get_first_codepoint(0), 0);
        assert_eq!(schar_get_ascii(0), 0);
        assert_eq!(schar_cells(0), 1);
    }

    #[test]
    fn line_do_arabic_shape_leaves_a_non_arabic_line_alone() {
        let _l = lock();
        let mut buf: Vec<ScharT> = "hello"
            .chars()
            .map(|c| schar_from_char(c as i32))
            .collect();
        let before = buf.clone();
        unsafe { line_do_arabic_shape(&mut buf) };
        assert_eq!(buf, before);
    }

    #[test]
    fn line_do_arabic_shape_leaves_an_empty_line_alone() {
        let _l = lock();
        let mut buf: Vec<ScharT> = Vec::new();
        unsafe { line_do_arabic_shape(&mut buf) };
        assert!(buf.is_empty());
    }

    #[test]
    fn schar_in_arabic_block_matches_only_the_arabic_lead_bytes() {
        let _l = lock();
        // U+0600..U+07FF encode with a 0xD8 or 0xD9 lead byte.
        assert!(schar_in_arabic_block(schar_from_char(0x0644))); // LAM
        assert!(schar_in_arabic_block(schar_from_char(0x0627))); // ALEF
        // Latin, and a box-drawing char (0xE2 lead), are not.
        assert!(!schar_in_arabic_block(schar_from_ascii(b'a')));
        assert!(!schar_in_arabic_block(schar_from_char(0x2500)));
    }

    #[test]
    fn schar_get_first_two_codepoints_splits_a_base_and_its_composing_char() {
        let _l = lock();
        // A bare character has no second codepoint.
        assert_eq!(
            schar_get_first_two_codepoints(schar_from_char(0x0644)),
            (0x0644, 0)
        );
        // A base plus one composing char reports both.
        let sc = schar_from_buf("\u{0644}\u{0627}".as_bytes());
        assert_eq!(schar_get_first_two_codepoints(sc), (0x0644, 0x0627));
        // The empty glyph reports nothing at all.
        assert_eq!(schar_get_first_two_codepoints(0), (0, 0));
    }

    #[test]
    fn line_do_arabic_shape_joins_a_run_of_lam() {
        let _l = lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let (prev_arshape, prev_tbidi) = (opts.p_arshape, opts.p_tbidi);
        opts.p_arshape = 1;
        opts.p_tbidi = 0;

        // Three LAMs in a row. Hand-traced against a_LAM's own ACHARS
        // row (isolated fedd, initial fedf, medial fee0, final fede)
        // and this function's visual-vs-logical argument mirroring:
        // the FIRST glyph has no visual predecessor but does have a
        // successor, which `arabic_shape` sees as its `prev_c`, so it
        // shapes as a FINAL form; the middle one joins both ways and
        // is MEDIAL; the last has only a visual predecessor, seen as
        // `next_c`, so it is INITIAL.
        let lam = schar_from_char(0x0644);
        let mut buf = vec![lam; 3];
        unsafe { line_do_arabic_shape(&mut buf) };

        assert_eq!(schar_get_first_two_codepoints(buf[0]).0, 0xfede);
        assert_eq!(schar_get_first_two_codepoints(buf[1]).0, 0xfee0);
        assert_eq!(schar_get_first_two_codepoints(buf[2]).0, 0xfedf);

        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_arshape = prev_arshape;
        opts.p_tbidi = prev_tbidi;
    }

    #[test]
    fn line_do_arabic_shape_gates_only_the_ligature_on_arabicshape() {
        let _l = lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let (prev_arshape, prev_tbidi) = (opts.p_arshape, opts.p_tbidi);

        // A LAM carrying a composing ALEF. With 'arabicshape' ON the
        // pair collapses into the LAM-ALEF ligature and the composing
        // char is consumed.
        let lam_alef = schar_from_buf("\u{0644}\u{0627}".as_bytes());
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_arshape = 1;
        opts.p_tbidi = 0;
        let mut buf = vec![lam_alef];
        unsafe { line_do_arabic_shape(&mut buf) };
        assert_eq!(schar_get_first_two_codepoints(buf[0]), (0xfefb, 0));

        // With it OFF the ligature does not form - but the ordinary
        // joining forms still apply, so the LAM becomes its ISOLATED
        // presentation form and keeps its composing char. Only
        // `arabic_combine` consults 'arabicshape'.
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_arshape = 0;
        let mut buf = vec![lam_alef];
        unsafe { line_do_arabic_shape(&mut buf) };
        assert_eq!(schar_get_first_two_codepoints(buf[0]), (0xfedd, 0x0627));

        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_arshape = prev_arshape;
        opts.p_tbidi = prev_tbidi;
    }

    /// Build a small backing store for a `ScreenGrid` test, returning
    /// the grid plus the vectors it borrows (which must outlive it).
    fn test_grid(
        rows: i32,
        cols: i32,
    ) -> (
        crate::grid_defs::ScreenGrid,
        Vec<ScharT>,
        Vec<crate::types_defs::SattrT>,
        Vec<crate::pos_defs::ColnrT>,
        Vec<usize>,
    ) {
        let n = (rows * cols) as usize;
        let mut chars: Vec<ScharT> = vec![0; n];
        let mut attrs: Vec<crate::types_defs::SattrT> = vec![7; n];
        let mut vcols: Vec<crate::pos_defs::ColnrT> = vec![0; n];
        let mut line_offset: Vec<usize> =
            (0..rows as usize).map(|r| r * cols as usize).collect();
        let grid = crate::grid_defs::ScreenGrid {
            chars: chars.as_mut_ptr(),
            attrs: attrs.as_mut_ptr(),
            vcols: vcols.as_mut_ptr(),
            line_offset: line_offset.as_mut_ptr(),
            rows,
            cols,
            ..Default::default()
        };
        (grid, chars, attrs, vcols, line_offset)
    }

    #[test]
    fn grid_clear_line_fills_spaces_and_marks_validity() {
        let _l = lock();
        let (mut grid, chars, attrs, vcols, _lo) = test_grid(2, 4);

        // A valid clear leaves attribute 0; an invalid one leaves -1.
        unsafe { grid_clear_line(&mut grid, 0, 4, true) };
        unsafe { grid_clear_line(&mut grid, 4, 4, false) };

        let space = schar_from_ascii(b' ');
        assert!(chars.iter().all(|&c| c == space));
        assert_eq!(&attrs[..4], &[0, 0, 0, 0]);
        assert_eq!(&attrs[4..], &[-1, -1, -1, -1]);
        // vcols is always invalidated, either way.
        assert!(vcols.iter().all(|&v| v == -1));
    }

    #[test]
    fn grid_invalidate_marks_every_attribute_invalid() {
        let _l = lock();
        let (mut grid, _c, attrs, _v, _lo) = test_grid(3, 5);
        assert!(attrs.iter().all(|&a| a == 7)); // the initial fill
        unsafe { grid_invalidate(&mut grid) };
        assert_eq!(attrs.len(), 15);
        assert!(attrs.iter().all(|&a| a == -1));
    }

    #[test]
    fn grid_getchar_reads_through_the_line_offset_table() {
        let _l = lock();
        let (mut grid, mut chars, mut attrs, _v, _lo) = test_grid(2, 4);
        // Row 1, column 2 is index 1*4 + 2 == 6.
        chars[6] = schar_from_ascii(b'z');
        attrs[6] = 42;
        let _ = &mut chars;
        let _ = &mut attrs;

        assert_eq!(
            unsafe { grid_getchar(&grid, 1, 2) },
            (schar_from_ascii(b'z'), 42)
        );

        // Out-of-range row or column returns the NUL character.
        assert_eq!(unsafe { grid_getchar(&grid, 2, 0) }, (0, 0));
        assert_eq!(unsafe { grid_getchar(&grid, 0, 4) }, (0, 0));

        // A grid with no backing store is also rejected.
        grid.chars = std::ptr::null_mut();
        assert_eq!(unsafe { grid_getchar(&grid, 0, 0) }, (0, 0));
    }

    #[test]
    fn schar_from_str_maps_none_to_zero() {
        let _l = lock();
        assert_eq!(schar_from_str(None), 0);
        assert_eq!(schar_from_str(Some(b"-")), schar_from_ascii(b'-'));
    }

    #[test]
    fn schar_from_buf_of_an_empty_slice_is_zero() {
        let _l = lock();
        // Zero is also what an all-NUL inline value decodes back to.
        assert_eq!(schar_from_buf(b""), 0);
        assert_eq!(schar_get(0), Vec::<u8>::new());
    }

    #[test]
    fn schar_high_discriminates_on_the_first_byte_in_memory_order() {
        // The two endianness branches must agree that the marker is
        // the first byte in memory order.
        let marked = if cfg!(target_endian = "big") {
            0xFF << 24
        } else {
            0xFFu32
        };
        assert!(schar_high(marked));
        assert!(!schar_high(schar_from_ascii(b'a')));
        assert_eq!(schar_idx(marked), 0);
    }
}
