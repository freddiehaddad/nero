//! Memory allocation allocator, string utilities, and linked-list mergesort
//! (`src/nvim/memory.c`).
#![forbid(unsafe_code)]

use crate::api::ui::remote_ui_free_all_mem;
use crate::buffer_updates::buf_free_callbacks;
use crate::errors::*;
use crate::ex_cmds::free_prev_shellcmd;
use crate::ex_docmd::{do_cmdline_cmd, free_cd_dir};
use crate::globals::*;
use crate::input::close_all_scripts;
use crate::lua::executor::nlua_free_all_mem;
use crate::main_init::preserve_exit;
use crate::memfile::mf_release_all;
use crate::message::{clear_sb_text, msg_hist_clear, reset_last_sourcing, set_keep_msg};
use crate::option::free_all_options;
use crate::option_vars::*;
use crate::register::set_expr_line;
use crate::ui::ui_free_all_mem;
use crate::ui_client::ui_client_free_all_mem;
use std::cmp::Ordering;
use std::ffi::{CStr, CString};

pub const ARENA_BLOCK_SIZE: usize = 4096;

pub(crate) struct MemoryModuleState {
    pub(crate) entered_free_all_mem: std::cell::Cell<bool>,
    trying_to_free: std::cell::Cell<bool>,
}

impl MemoryModuleState {
    const fn new() -> Self {
        Self {
            entered_free_all_mem: std::cell::Cell::new(false),
            trying_to_free: std::cell::Cell::new(false),
        }
    }
}

thread_local! {
    static MEM_STATE: MemoryModuleState = const { MemoryModuleState::new() };
}

#[inline(always)]
pub(crate) fn entered_free_all_mem() -> bool {
    MEM_STATE.with(|s| s.entered_free_all_mem.get())
}

pub(crate) fn try_to_free_memory() {
    if MEM_STATE.with(|s| s.trying_to_free.get()) {
        return;
    }
    MEM_STATE.with(|s| s.trying_to_free.set(true));

    clear_sb_text(true);
    mf_release_all();

    MEM_STATE.with(|s| s.trying_to_free.set(false));
}

pub(crate) fn out_of_memory_exit() -> ! {
    preserve_exit(Some(E_OUTOFMEM))
}

/// Safe O(1) sub-slicing of a `&CStr`.
pub trait CStrSubslice<'a> {
    fn advance(self, n: usize) -> &'a CStr;
}

impl<'a> CStrSubslice<'a> for &'a CStr {
    #[inline(always)]
    fn advance(self, n: usize) -> &'a CStr {
        let len = self.to_bytes().len();
        &self[n.min(len)..]
    }
}

impl<'a> CStrSubslice<'a> for &&'a CStr {
    #[inline(always)]
    fn advance(self, n: usize) -> &'a CStr {
        (*self).advance(n)
    }
}

impl<'a> CStrSubslice<'a> for &'a crate::option_vars::RcCStr {
    #[inline(always)]
    fn advance(self, n: usize) -> &'a CStr {
        self.as_cstr().advance(n)
    }
}

impl<'a> CStrSubslice<'a> for &&'a crate::option_vars::RcCStr {
    #[inline(always)]
    fn advance(self, n: usize) -> &'a CStr {
        self.as_cstr().advance(n)
    }
}

impl<'a> CStrSubslice<'a> for &mut &'a CStr {
    #[inline(always)]
    fn advance(self, n: usize) -> &'a CStr {
        (*self).advance(n)
    }
}

/// View a byte slice up to the first NUL byte (or the whole slice if no NUL is present).
#[inline(always)]
pub fn bytes_until_nul(bytes: &[u8]) -> &[u8] {
    CStr::from_bytes_until_nul(bytes).map_or(bytes, CStr::to_bytes)
}

/// Where `part` starts in `whole`, or `None` when it does not start within
/// `whole` (its end included). Unlike `whole.len() - part.len()`, this does not
/// require the two slices to end in the same place.
#[inline]
pub fn offset_in(whole: &[u8], part: &[u8]) -> Option<usize> {
    let offset = (part.as_ptr() as usize).checked_sub(whole.as_ptr() as usize)?;
    (offset <= whole.len()).then_some(offset)
}

/// View a byte slice up to the first NUL byte as a `&CStr` (or `c""` if no NUL is present).
#[inline(always)]
pub fn bytes_as_cstr(bytes: &[u8]) -> &CStr {
    CStr::from_bytes_until_nul(bytes).unwrap_or(c"")
}

/// Clone a byte slice up to the first NUL byte into an owned `CString`.
#[inline(always)]
pub fn cstring_until_nul(bytes: &[u8]) -> CString {
    bytes_as_cstr(bytes).to_owned()
}

/// Safely mutate the bytes of a `CString` in place via its owned `Vec<u8>`.
#[inline(always)]
pub fn cstring_modify<R>(s: &mut CString, f: impl FnOnce(&mut [u8]) -> R) -> R {
    let mut v = std::mem::take(s).into_bytes_with_nul();
    let r = f(&mut v);
    *s = cstring_until_nul(&v);
    r
}

/// Copy `src` (up to any embedded NUL) into sized C buffer `dst`, NUL-terminating `dst`.
/// Returns the byte length of `src` (up to any embedded NUL), matching `strlcpy` semantics.
#[inline(always)]
pub fn copy_bytes_to_c_buf(dst: &mut [u8], src: &[u8]) -> usize {
    let src = bytes_until_nul(src);
    let slen = src.len();
    if !dst.is_empty() {
        let len = slen.min(dst.len() - 1);
        dst[..len].copy_from_slice(&src[..len]);
        dst[len] = 0;
    }
    slen
}

/// Append `src` (up to any embedded NUL) onto NUL-terminated C buffer `dst`, NUL-terminating `dst`.
/// Returns the byte length of `src` (up to any embedded NUL).
#[inline(always)]
pub fn append_bytes_to_c_buf(dst: &mut [u8], src: &[u8]) -> usize {
    let dlen = bytes_until_nul(dst).len();
    if dlen < dst.len() {
        copy_bytes_to_c_buf(&mut dst[dlen..], src)
    } else {
        bytes_until_nul(src).len()
    }
}

/// Copy a byte slice (truncating at any embedded NUL) into an owned `CString`.
pub fn bytes_to_cstring(bytes: &[u8]) -> CString {
    match CStr::from_bytes_until_nul(bytes) {
        Ok(c) => c.to_owned(),
        Err(_) => CString::new(bytes).unwrap_or_default(),
    }
}

/// Convert an owned `Vec<u8>` (truncating at any embedded NUL) into a `CString` without copying.
#[inline]
pub fn vec_to_cstring(mut v: Vec<u8>) -> CString {
    if let Some(pos) = v.iter().position(|&b| b == 0) {
        v.truncate(pos + 1);
    } else {
        v.push(0);
    }
    CString::from_vec_with_nul(v).unwrap_or_default()
}

/// Sorts `v` by `compare` the way glibc's `qsort()` does, which upstream uses
/// for `sort()`: a stable merge sort that halves each run and takes the left
/// item unless `compare` puts it after the right one. Unlike
/// `slice::sort_by()`, which may panic when `compare` is not a total order,
/// it accepts any `compare`, such as a user's compare function, and then
/// orders the items as upstream does on Linux.
pub fn qsort_by<T>(v: &mut [T], mut compare: impl FnMut(&T, &T) -> Ordering) {
    let mut order: Vec<usize> = (0..v.len()).collect();
    let mut scratch = vec![0; v.len()];
    sort_halves(&mut order, &mut scratch, &mut |a, b| compare(&v[a], &v[b]));
    permute(v, &mut order);
}

/// Sorts `v` by `compare` like upstream's `mergesort_list()`: a stable merge
/// sort that merges runs of 1, 2, 4, ... items from the left, taking the left
/// item unless `compare` puts it after the right one. Like `qsort_by()`, it
/// accepts a `compare` that is not a total order.
pub fn mergesort_list<T>(v: &mut [T], mut compare: impl FnMut(&T, &T) -> Ordering) {
    let len = v.len();
    let mut order: Vec<usize> = (0..len).collect();
    let mut scratch = vec![0; len];
    let mut compare_at = |a: usize, b: usize| compare(&v[a], &v[b]);
    let mut size = 1;
    while size < len {
        for start in (0..len).step_by(2 * size) {
            let end = (start + 2 * size).min(len);
            let mid = (start + size).min(end);
            merge(
                &mut order[start..end],
                mid - start,
                &mut scratch,
                &mut compare_at,
            );
        }
        size *= 2;
    }
    permute(v, &mut order);
}

/// Sorts the item indices in `run` as glibc's merge sort does.
fn sort_halves(
    run: &mut [usize],
    scratch: &mut [usize],
    compare: &mut impl FnMut(usize, usize) -> Ordering,
) {
    if run.len() <= 1 {
        return;
    }
    let mid = run.len() / 2;
    sort_halves(&mut run[..mid], scratch, compare);
    sort_halves(&mut run[mid..], scratch, compare);
    merge(run, mid, scratch, compare);
}

/// Merges the sorted item indices `run[..mid]` and `run[mid..]`, taking the
/// left item unless `compare` puts it after the right one.
fn merge(
    run: &mut [usize],
    mid: usize,
    scratch: &mut [usize],
    compare: &mut impl FnMut(usize, usize) -> Ordering,
) {
    let (mut left, mut right, mut merged) = (0, mid, 0);
    while left < mid && right < run.len() {
        if compare(run[left], run[right]) == Ordering::Greater {
            scratch[merged] = run[right];
            right += 1;
        } else {
            scratch[merged] = run[left];
            left += 1;
        }
        merged += 1;
    }
    // The rest of the right run is in place; the rest of the left run goes
    // before it.
    let end = merged + mid - left;
    scratch[merged..end].copy_from_slice(&run[left..mid]);
    run[..end].copy_from_slice(&scratch[..end]);
}

/// Moves the item at `order[k]` to `k`, for each `k`, where `order` holds each
/// index of `v` once.
fn permute<T>(v: &mut [T], order: &mut [usize]) {
    for start in 0..v.len() {
        // Follow the cycle through `start`, moving each item into place.
        let mut k = start;
        while order[k] != start {
            let from = order[k];
            v.swap(k, from);
            order[k] = k;
            k = from;
        }
        order[k] = k;
    }
}

/// Free everything that we allocated (`free_all_mem`).
pub fn free_all_mem() {
    use crate::api::extmark::api_extmark_free_all_mem;
    use crate::arglist::alist_clear;
    use crate::autocmd::{autocmd_free_all_mem, block_autocmds, free_all_autocmds};
    use crate::buffer::{Bufref, DOBUF_WIPE, bufref_valid, close_buffer, free_titles, set_bufref};
    use crate::channel::channel_free_all_mem;
    use crate::cmdhist::init_history;
    use crate::decoration_provider::decor_free_all_mem;
    use crate::diff::diff_clear;
    use crate::drawline::drawline_free_all_mem;
    use crate::eval::eval_clear;
    use crate::ex_cmds::free_old_sub;
    use crate::ex_docmd::Exarg;
    use crate::file_search::free_findfile;
    use crate::grid::grid_free_all_mem;
    use crate::highlight::clear_hl_tables;
    use crate::highlight_group::free_highlight;
    use crate::input::redo_free_all;
    use crate::input_cmdatom::atom_free_all;
    use crate::insert::free_last_insert;
    use crate::insexpand::free_insexpand_stuff;
    use crate::mapping::map_clear_mode;
    use crate::mark::free_all_marks;
    use crate::mcursor::mc_free_all;

    use crate::msgpack_rpc::channel::rpc_free_all_mem;
    use crate::os::env::free_homedir;
    use crate::os::users::free_users;
    use crate::quickfix::{check_quickfix_busy, free_quickfix};
    use crate::regexp::free_regexp_stuff;
    use crate::register::clear_registers;
    use crate::search::free_search_patterns;
    use crate::sign::free_signs;
    use crate::spell::spell_free_all;
    use crate::state::MAP_ALL_MODES;
    use crate::statusline::stl_clear_click_defs;
    use crate::tag::free_tag_stuff;
    use crate::ui_compositor::ui_comp_free_all_mem;
    use crate::usercmd::ex_comclear;
    use crate::window::{free_tabpage, win_free_all};

    {
        if MEM_STATE.with(|s| s.entered_free_all_mem.get()) {
            nlua_free_all_mem();
            return;
        }
        MEM_STATE.with(|s| s.entered_free_all_mem.set(true));
        block_autocmds();

        opt_vars().p_ea.set(false);
        if !workspace_state().first_tabpage.get().is_null()
            && !crate::globals::first_tabpage().tp_next.get().is_null()
        {
            do_cmdline_cmd(b"tabonly!");
        }

        spell_free_all();
        ex_comclear(&mut Exarg::new());

        if !workspace_state().curbuf.get().is_null() {
            do_cmdline_cmd(b"aunmenu *");
            do_cmdline_cmd(b"tlunmenu *");
            do_cmdline_cmd(b"menutranslate clear");

            map_clear_mode(&curbuf(), MAP_ALL_MODES, false, false);
            map_clear_mode(&curbuf(), MAP_ALL_MODES, false, true);
            do_cmdline_cmd(b"breakdel *");
            do_cmdline_cmd(b"profdel *");
            do_cmdline_cmd(b"set keymap=");
        }

        free_titles();
        free_findfile();

        free_all_autocmds();
        free_all_marks();
        alist_clear(&crate::arglist::global_alist_rc());
        free_homedir();
        free_users();
        free_search_patterns();
        free_old_sub();
        free_last_insert();
        free_insexpand_stuff();
        free_prev_shellcmd();
        free_regexp_stuff();
        free_tag_stuff();
        free_cd_dir();
        free_signs();
        set_expr_line(None);
        if !workspace_state().curtab.get().is_null() {
            diff_clear(&curtab());
        }
        clear_sb_text(true);

        cmdline_state().last_cmdline.set(None);
        cmdline_state().new_last_cmdline.set(None);
        set_keep_msg(None, 0);

        opt_vars().p_hi.set(0);
        init_history();

        free_quickfix();
        close_all_scripts();

        if !(curwin()).is_null() {
            win_free_all();
        }

        free_all_options();

        opt_vars().p_acd.set(false);
        let mut buf = workspace_state().firstbuf.get();
        while !buf.is_null() {
            let mut bufref = Bufref {
                br_buf: crate::globals::BufPtr::NULL,
                br_fnum: 0,
                br_buf_free_count: 0,
            };
            set_bufref(&mut bufref, buf.opt());
            let nextbuf = buf.b_next.get();
            buf.b_nwindows.set(0);

            buf_free_callbacks(buf.borrow());
            close_buffer(None, buf.opt(), DOBUF_WIPE, false, false, false);
            buf = if bufref_valid(Some(&bufref)) {
                nextbuf
            } else {
                workspace_state().firstbuf.get()
            };
        }

        clear_registers();
        redo_free_all();
        free_highlight();
        reset_last_sourcing();

        if !workspace_state().first_tabpage.get().is_null() {
            let first_tp = workspace_state().first_tabpage.get().clone();
            free_tabpage(first_tp.opt());
            workspace_state()
                .first_tabpage
                .set(crate::globals::TabpagePtr::NULL);
            workspace_state()
                .curtab
                .set(crate::globals::TabpagePtr::NULL);
        }

        msg_hist_clear(0);

        // Queued requests retain channels and their Lua/Vimscript callbacks.
        rpc_free_all_mem();
        channel_free_all_mem();
        eval_clear();
        api_extmark_free_all_mem();
        atom_free_all();
        mc_free_all();

        workspace_state().clear_handles();

        grid_free_all_mem();
        screen_state().with_tab_page_click_defs_mut(|defs| stl_clear_click_defs(defs));
        screen_state().clear_tab_page_click_defs();

        clear_hl_tables(false);
        check_quickfix_busy();
        decor_free_all_mem();
        drawline_free_all_mem();

        if screen_state().ui_client_channel_id.get() != 0 {
            ui_client_free_all_mem();
        }

        remote_ui_free_all_mem();
        ui_free_all_mem();
        ui_comp_free_all_mem();
        nlua_free_all_mem();
        autocmd_free_all_mem();
    }
}

#[derive(Clone, Default)]
pub struct FxHasher {
    hash: u64,
}

impl FxHasher {
    const K: u64 = 0x517c_c1b7_2722_0a95;

    #[inline(always)]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(Self::K);
    }
}

impl std::hash::Hasher for FxHasher {
    #[inline(always)]
    fn finish(&self) -> u64 {
        self.hash
    }

    #[inline(always)]
    fn write(&mut self, mut bytes: &[u8]) {
        while let Some((&chunk, rest)) = bytes.split_first_chunk::<8>() {
            self.add(u64::from_ne_bytes(chunk));
            bytes = rest;
        }
        if let Some((&chunk, rest)) = bytes.split_first_chunk::<4>() {
            self.add(u32::from_ne_bytes(chunk) as u64);
            bytes = rest;
        }
        if let Some((&chunk, rest)) = bytes.split_first_chunk::<2>() {
            self.add(u16::from_ne_bytes(chunk) as u64);
            bytes = rest;
        }
        if let Some(&b) = bytes.first() {
            self.add(b as u64);
        }
    }

    #[inline(always)]
    fn write_u8(&mut self, i: u8) {
        self.add(i as u64);
    }

    #[inline(always)]
    fn write_u16(&mut self, i: u16) {
        self.add(i as u64);
    }

    #[inline(always)]
    fn write_u32(&mut self, i: u32) {
        self.add(i as u64);
    }

    #[inline(always)]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }

    #[inline(always)]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }

    #[inline(always)]
    fn write_i8(&mut self, i: i8) {
        self.add(i as u8 as u64);
    }

    #[inline(always)]
    fn write_i16(&mut self, i: i16) {
        self.add(i as u16 as u64);
    }

    #[inline(always)]
    fn write_i32(&mut self, i: i32) {
        self.add(i as u32 as u64);
    }

    #[inline(always)]
    fn write_i64(&mut self, i: i64) {
        self.add(i as u64);
    }

    #[inline(always)]
    fn write_isize(&mut self, i: isize) {
        self.add(i as usize as u64);
    }
}

pub type FxBuildHasher = std::hash::BuildHasherDefault<FxHasher>;
pub type FxHashMap<K, V> = std::collections::HashMap<K, V, FxBuildHasher>;
pub type FxHashSet<T> = std::collections::HashSet<T, FxBuildHasher>;

#[inline(always)]
pub const fn fx_hashmap<K, V>() -> FxHashMap<K, V> {
    std::collections::HashMap::with_hasher(std::hash::BuildHasherDefault::new())
}

#[inline(always)]
pub const fn fx_hashset<T>() -> FxHashSet<T> {
    std::collections::HashSet::with_hasher(std::hash::BuildHasherDefault::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Xorshift generator with a fixed seed, so failures reproduce.
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: u64) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 % n
        }
    }

    type Item = (u64, usize);
    type Sort = fn(&mut [Item], &mut dyn FnMut(&Item, &Item) -> Ordering);

    /// The sorts that accept any comparator.
    const SORTS: [(&str, Sort); 2] = [
        ("qsort_by", |v, compare| qsort_by(v, compare)),
        ("mergesort_list", |v, compare| mergesort_list(v, compare)),
    ];

    #[test]
    fn sorts_are_stable() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        for (name, sort) in SORTS {
            for len in (0..70).chain([257, 1000]) {
                // Few distinct keys, so that most items have equal ones.
                let items: Vec<Item> = (0..len).map(|i| (rng.below(8), i)).collect();
                let mut expected = items.clone();
                expected.sort_by_key(|&(key, _)| key);
                let mut sorted = items;
                sort(&mut sorted, &mut |a, b| a.0.cmp(&b.0));
                assert_eq!(sorted, expected, "{name}, {len} items");
            }
        }
    }

    #[test]
    fn sorts_accept_any_comparator() {
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        for (name, sort) in SORTS {
            // A comparator that answers at random still leaves each item once.
            for len in [2, 21, 100, 1000] {
                let mut items: Vec<Item> = (0..len).map(|i| (0, i)).collect();
                sort(&mut items, &mut |_, _| match rng.below(3) {
                    0 => Ordering::Less,
                    1 => Ordering::Equal,
                    _ => Ordering::Greater,
                });
                items.sort_unstable();
                assert_eq!(
                    items,
                    (0..len).map(|i| (0, i)).collect::<Vec<_>>(),
                    "{name}"
                );
            }
            // One that never answers `Less`, like `{a, b -> a > b}`, still sorts.
            let mut items: Vec<Item> = [5, 3, 9, 1, 7, 2, 8, 6, 4].map(|k| (k, 0)).to_vec();
            sort(&mut items, &mut |a, b| {
                if a.0 > b.0 {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            });
            let keys: Vec<u64> = items.iter().map(|&(k, _)| k).collect();
            assert_eq!(keys, [1, 2, 3, 4, 5, 6, 7, 8, 9], "{name}");
        }
    }

    #[test]
    fn offset_in_finds_parts_that_end_elsewhere() {
        let text = b"abc\0def";
        assert_eq!(offset_in(text, &text[2..]), Some(2));
        assert_eq!(offset_in(text, &text[2..3]), Some(2));
        assert_eq!(offset_in(&text[..3], &text[2..]), Some(2));
        assert_eq!(offset_in(&text[..3], &text[3..]), Some(3));
        assert_eq!(offset_in(&text[..3], &text[4..]), None);
        assert_eq!(offset_in(&text[2..], text), None);
    }

    #[test]
    fn fx_hash_collections_work() {
        let mut map: FxHashMap<u64, usize> = fx_hashmap();
        let mut set: FxHashSet<Vec<u8>> = fx_hashset();
        for i in 0..128u64 {
            map.insert(i * 17, i as usize);
            set.insert(format!("key_{i}_long_suffix").into_bytes());
        }
        for i in 0..128u64 {
            assert_eq!(map.get(&(i * 17)), Some(&(i as usize)));
            assert!(set.contains(format!("key_{i}_long_suffix").as_bytes()));
        }
    }
}
