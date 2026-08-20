//! Translated from `src/nvim/os/mem.c` in full.

/// Total system physical memory in KiB (`os_get_total_mem_kib`).
#[must_use]
pub fn os_get_total_mem_kib() -> u64 {
    total_memory_bytes() / 1024
}

#[cfg(unix)]
fn total_memory_bytes() -> u64 {
    // SAFETY: sysconf has no pointer arguments and these selectors
    // return process-independent scalar system values.
    let pages = unsafe { libc::sysconf(libc::_SC_PHYS_PAGES) };
    // SAFETY: same as above.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if pages <= 0 || page_size <= 0 {
        return 0;
    }
    (pages as u64).saturating_mul(page_size as u64)
}

#[cfg(windows)]
fn total_memory_bytes() -> u64 {
    #[repr(C)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
    }

    let mut status = MemoryStatusEx {
        length: std::mem::size_of::<MemoryStatusEx>() as u32,
        memory_load: 0,
        total_phys: 0,
        avail_phys: 0,
        total_page_file: 0,
        avail_page_file: 0,
        total_virtual: 0,
        avail_virtual: 0,
        avail_extended_virtual: 0,
    };
    // SAFETY: status is correctly sized, initialized, and exclusively
    // accessible for the duration of the call.
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
        return 0;
    }
    status.total_phys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call sysconf/GlobalMemoryStatusEx")]
    fn total_memory_is_nonzero_on_the_test_host() {
        assert!(os_get_total_mem_kib() > 0);
    }
}
