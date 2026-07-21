//! Translated from `src/nvim/memory.c` ("Various routines dealing with
//! allocation and deallocation of memory").
//!
//! Deferred (real forward dependencies on subsystems not yet translated -
//! not faked, explicitly not yet started):
//! - `try_to_free_memory`/`do_outofmem_msg`/`try_malloc`/`verbose_try_malloc`:
//!   need `mf_release_all` (`memfile.c`, phase 3), `clear_sb_text`
//!   (terminal.c, phase 14), `arena_free_reuse_blks` (this file's arena
//!   allocator, see below), and `semsg`/`emsg_silent` (`message.c`, phase
//!   15).
//! - The arena bump allocator (`alloc_block`, `arena_alloc_block`,
//!   `arena_align_offset`, `arena_alloc`, `free_block`, `arena_mem_free`,
//!   `arena_finish`): a from-scratch custom allocator doing manual pointer
//!   arithmetic and block-header tricks in the original; translating it
//!   properly needs focused `unsafe` Rust and is deferred to its own pass
//!   rather than being rushed alongside the simpler string utilities below.
//! - `mergesort_list`: a generic intrusive-doubly-linked-list merge sort
//!   (`void*` + get/set next/prev function pointers in the original, C's
//!   way of writing generic code). Deferred until a real caller is
//!   translated, so the Rust generic bound (trait) shape matches actual
//!   usage instead of being guessed.
//!
//! `xmalloc`/`xcalloc`/`xrealloc`/`xfree`/`xmallocz` below implement the
//! *outer* contract ("never returns null; aborts on OOM") using Rust's
//! default allocator, which already aborts the process on allocation
//! failure (`std::alloc::handle_alloc_error`) - satisfying the same
//! contract as the original. What's missing is the original's *first*
//! step of trying `try_to_free_memory()` before giving up; that retry is
//! deferred along with `try_to_free_memory` itself, per above.
//!
//! `xstrnlen` is skipped: the original only defines it
//! `#ifndef HAVE_STRNLEN`, a fallback for libcs lacking `strnlen()`
//! (`auto/config.h` feature detection) - dead code on every modern target.

use crate::ascii_defs::NUL;

/// `xmalloc`: never returns null (aborts via Rust's allocator on OOM,
/// matching the original's ultimate `preserve_exit` guarantee - but without
/// the original's `try_to_free_memory()` retry step first; see module docs).
#[inline]
pub fn xmalloc(size: usize) -> Vec<u8> {
    vec![0u8; if size == 0 { 1 } else { size }]
}

/// `xcalloc`
#[inline]
pub fn xcalloc(count: usize, size: usize) -> Vec<u8> {
    let total = if count == 0 || size == 0 { 1 } else { count * size };
    vec![0u8; total]
}

/// `xrealloc`
#[inline]
pub fn xrealloc(mut ptr: Vec<u8>, size: usize) -> Vec<u8> {
    let new_size = if size == 0 { 1 } else { size };
    ptr.resize(new_size, 0);
    ptr
}

/// `xfree`: a no-op in Rust - dropping the `Vec`/`Box` frees it. Kept as a
/// function so translated call sites (`xfree(x)`) still have somewhere to
/// go; real callers will most likely just drop the value and never call
/// this once fully translated.
#[inline]
pub fn xfree<T>(_ptr: T) {}

/// `xmallocz`: `xmalloc` wrapper that allocates `size + 1` bytes and zeroes
/// the last byte (commonly used for NUL-terminated strings, when a
/// translated call site actually needs a NUL-terminated buffer for further
/// C-string-style processing).
#[inline]
pub fn xmallocz(size: usize) -> Vec<u8> {
    let total_size = size
        .checked_add(1)
        .unwrap_or_else(|| panic!("Nvim: Data too large to fit into virtual memory space"));
    let mut ret = xmalloc(total_size);
    ret[size] = NUL;
    ret
}

/// Allocates `len + 1` bytes, copies `len` bytes of `data`, NUL-terminates
/// (`xmemdupz`).
#[inline]
pub fn xmemdupz(data: &[u8]) -> Vec<u8> {
    let mut ret = xmallocz(data.len());
    ret[..data.len()].copy_from_slice(data);
    ret
}

/// Copies `src` into `dst` (which must be at least `src.len() + 1` bytes)
/// and NUL-terminates it (`xmemcpyz`).
///
/// # Safety
/// `dst` must have room for at least `src.len() + 1` bytes, matching the
/// original's undocumented (caller-guaranteed) contract.
#[inline]
pub fn xmemcpyz(dst: &mut [u8], src: &[u8]) {
    dst[..src.len()].copy_from_slice(src);
    dst[src.len()] = NUL;
}

/// A version of `memchr` that returns one past the end if it doesn't find
/// `c` (`xmemscan`).
#[inline]
pub fn xmemscan(data: &[u8], c: u8) -> usize {
    memchr(data, c).unwrap_or(data.len())
}

/// Small local helper mirroring C's `memchr` on a slice (returns index, not
/// pointer, since Rust slices aren't addressed by raw pointer here).
#[inline]
fn memchr(data: &[u8], c: u8) -> Option<usize> {
    data.iter().position(|&b| b == c)
}

/// Replaces every instance of `c` with `x` (`strchrsub`/`memchrsub` are
/// unified here: Rust slices already carry their length, so there is no
/// separate "NUL-terminated string" overload needed - callers pass the
/// exact byte range to operate on).
#[inline]
pub fn memchrsub(data: &mut [u8], c: u8, x: u8) {
    for b in data.iter_mut() {
        if *b == c {
            *b = x;
        }
    }
}

/// Counts the number of occurrences of byte `c` in `data` (`strcnt`/`memcnt`
/// unified for the same reason as `memchrsub` above).
#[inline]
pub fn memcnt(data: &[u8], c: u8) -> usize {
    data.iter().filter(|&&b| b == c).count()
}

/// A version of `memchr` that starts the search from the end
/// (`xmemrchr`). Based on glibc's `memrchr`.
#[inline]
pub fn xmemrchr(src: &[u8], c: u8) -> Option<usize> {
    src.iter().rposition(|&b| b == c)
}

/// `strdup()` wrapper (`xstrdup`).
#[inline]
pub fn xstrdup(s: &[u8]) -> Vec<u8> {
    xmemdupz(s)
}

/// `strdup()` wrapper; allocates a new empty string if given `None`
/// (`xstrdupnul`).
#[inline]
pub fn xstrdupnul(s: Option<&[u8]>) -> Vec<u8> {
    match s {
        None => xmallocz(0),
        Some(s) => xstrdup(s),
    }
}

/// `strndup()` wrapper (`xstrndup`): duplicates at most `len` bytes of
/// `str`, stopping earlier at an embedded NUL, matching the original's
/// `memchr(str, NUL, len)` short-circuit.
#[inline]
pub fn xstrndup(s: &[u8], len: usize) -> Vec<u8> {
    let bound = s.len().min(len);
    let slice = &s[..bound];
    let stop = memchr(slice, NUL).unwrap_or(bound);
    xmemdupz(&slice[..stop])
}

/// Duplicates a chunk of memory (`xmemdup`).
#[inline]
pub fn xmemdup(data: &[u8]) -> Vec<u8> {
    data.to_vec()
}

/// Returns true if strings `a` and `b` are equal. Arguments may be absent
/// (`strequal`; `Option<&[u8]>` stands in for the original's nullable
/// `const char *`).
#[inline]
pub fn strequal(a: Option<&[u8]>, b: Option<&[u8]>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Returns true if the first `n` bytes of `a` and `b` are equal. Arguments
/// may be absent (`strnequal`).
///
/// Note: `a`/`b` are modeled as exact string content (like the rest of this
/// module), not raw buffers possibly containing an embedded NUL before
/// their end - so unlike C's `strncmp`, this does not stop early at an
/// embedded NUL byte. This matches how every other function in this module
/// represents strings (as exact-length byte slices, no trailing NUL
/// needed), and no real caller exists yet to confirm this can't come up in
/// practice.
#[inline]
pub fn strnequal(a: Option<&[u8]>, b: Option<&[u8]>, n: usize) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let an = &a[..a.len().min(n)];
            let bn = &b[..b.len().min(n)];
            an == bn && an.len() == bn.len()
        }
        _ => false,
    }
}

/// Writes `time_t` to `buf[8]`, big-endian (`time_to_bytes`).
#[inline]
pub fn time_to_bytes(time_: i64, buf: &mut [u8; 8]) {
    for (i, b) in buf.iter_mut().enumerate() {
        let shift = (7 - i) * 8;
        *b = ((time_ as u64) >> shift) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xmallocz_nul_terminates() {
        let v = xmallocz(3);
        assert_eq!(v.len(), 4);
        assert_eq!(v[3], 0);
    }

    #[test]
    fn xmemdupz_copies_and_terminates() {
        let v = xmemdupz(b"abc");
        assert_eq!(&v[..3], b"abc");
        assert_eq!(v[3], 0);
    }

    #[test]
    fn xmemcpyz_copies_and_terminates() {
        let mut dst = vec![0u8; 5];
        xmemcpyz(&mut dst, b"ab");
        assert_eq!(&dst[..2], b"ab");
        assert_eq!(dst[2], 0);
    }

    #[test]
    fn xmemscan_finds_or_returns_len() {
        assert_eq!(xmemscan(b"abc", b'b'), 1);
        assert_eq!(xmemscan(b"abc", b'z'), 3);
    }

    #[test]
    fn memchrsub_replaces_all() {
        let mut data = b"a.b.c".to_vec();
        memchrsub(&mut data, b'.', b'-');
        assert_eq!(&data, b"a-b-c");
    }

    #[test]
    fn memcnt_counts_occurrences() {
        assert_eq!(memcnt(b"aabaa", b'a'), 4);
        assert_eq!(memcnt(b"aabaa", b'b'), 1);
    }

    #[test]
    fn xmemrchr_finds_from_end() {
        assert_eq!(xmemrchr(b"abcabc", b'a'), Some(3));
        assert_eq!(xmemrchr(b"abcabc", b'z'), None);
    }

    #[test]
    fn xstrndup_stops_at_embedded_nul() {
        let v = xstrndup(b"ab\0cd", 5);
        assert_eq!(&v[..2], b"ab");
        assert_eq!(v[2], 0);
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn strequal_handles_none() {
        assert!(strequal(None, None));
        assert!(!strequal(None, Some(b"a")));
        assert!(strequal(Some(b"a"), Some(b"a")));
        assert!(!strequal(Some(b"a"), Some(b"b")));
    }

    #[test]
    fn strnequal_bounds_by_n() {
        assert!(strnequal(Some(b"abcdef"), Some(b"abcxyz"), 3));
        assert!(!strnequal(Some(b"abcdef"), Some(b"abcxyz"), 4));
    }

    #[test]
    fn time_to_bytes_is_big_endian() {
        let mut buf = [0u8; 8];
        time_to_bytes(0x0102_0304_0506_0708, &mut buf);
        assert_eq!(buf, [1, 2, 3, 4, 5, 6, 7, 8]);
    }
}
