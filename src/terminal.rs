//! Translated from `src/nvim/terminal.c` (tractable core only).
//!
//! `terminal.c` (~2900 lines) implements the `:terminal` buffer: a
//! libvterm screen driven by a PTY, wired into neovim's buffer,
//! window, redraw and event-loop machinery. Nearly all of it depends
//! on the `Terminal`/`VTerm*` types (libvterm is a C library this
//! crate does not bind), the event loop, or the channel layer - none
//! translated.
//!
//! Translated: [`is_filter_char`] - a pure classification of one
//! character against the `'termpastefilter'` option flags, needing
//! only [`crate::option_vars`]'s already-real `tpf_flags` and
//! `opt_tpf_flag` constants, with no dependency on any terminal
//! state at all.
//!
//! Deferred: everything else - the terminal lifecycle
//! (`terminal_open`/`terminal_close`/`terminal_destroy`), input and
//! output (`terminal_send`/`terminal_receive`/`terminal_paste`), the
//! libvterm screen callbacks, and the redraw/cursor integration.

use crate::option_vars::opt_tpf_flag;

/// Whether character `c` should be filtered out of a terminal paste,
/// according to the `'termpastefilter'` option (`is_filter_char`).
///
/// Carriage return (`0x0D`) and line feed (`0x0A`) are never
/// filtered: they map to no flag at all, so the final test is against
/// a zero mask and always fails. That is the original's behaviour and
/// is preserved deliberately - a paste must keep its line structure.
///
/// # Safety
/// Reads `crate::option_vars::OPTION_VARS` - the same requirement as
/// every other function that does so.
#[must_use]
pub unsafe fn is_filter_char(c: i32) -> bool {
    let flag: u32 = match c {
        0x08 => opt_tpf_flag::BS,
        0x09 => opt_tpf_flag::HT,
        // Line feed and carriage return are never filtered.
        0x0A | 0x0D => 0,
        0x0C => opt_tpf_flag::FF,
        0x1b => opt_tpf_flag::ESC,
        0x7F => opt_tpf_flag::DEL,
        _ => {
            if c < b' '.into() {
                opt_tpf_flag::C0
            } else if (0x80..=0x9F).contains(&c) {
                opt_tpf_flag::C1
            } else {
                0
            }
        }
    };
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { crate::option_vars::OPTION_VARS.get_mut() }.tpf_flags & flag) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sets `'termpastefilter'`'s parsed flags, restoring the previous
    /// value on drop.
    struct TpfGuard {
        prev: u32,
    }

    impl TpfGuard {
        fn set(flags: u32) -> Self {
            let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let me = Self { prev: ov.tpf_flags };
            ov.tpf_flags = flags;
            me
        }
    }

    impl Drop for TpfGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.tpf_flags = self.prev;
        }
    }

    /// With no flags set nothing is filtered, whatever the character.
    #[test]
    fn is_filter_char_filters_nothing_when_no_flags_are_set() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = TpfGuard::set(0);
        for c in [0x08, 0x09, 0x0C, 0x1b, 0x7F, 0x01, 0x85, b'a'.into()] {
            assert!(!unsafe { is_filter_char(c) }, "char {c:#x} must not filter");
        }
    }

    /// Each named control character is filtered by its OWN flag and
    /// not by any other, so the mapping cannot be silently transposed.
    #[test]
    fn is_filter_char_maps_each_control_to_its_own_flag() {
        let _lock = crate::globals::global_state_test_lock();
        for (c, flag) in [
            (0x08, opt_tpf_flag::BS),
            (0x09, opt_tpf_flag::HT),
            (0x0C, opt_tpf_flag::FF),
            (0x1b, opt_tpf_flag::ESC),
            (0x7F, opt_tpf_flag::DEL),
        ] {
            let _g = TpfGuard::set(flag);
            assert!(unsafe { is_filter_char(c) }, "{c:#x} must filter under its flag");

            // Every other flag must leave it alone.
            for other in [
                opt_tpf_flag::BS,
                opt_tpf_flag::HT,
                opt_tpf_flag::FF,
                opt_tpf_flag::ESC,
                opt_tpf_flag::DEL,
            ] {
                if other == flag {
                    continue;
                }
                let _g2 = TpfGuard::set(other);
                assert!(
                    !unsafe { is_filter_char(c) },
                    "{c:#x} must not filter under flag {other:#x}"
                );
            }
        }
    }

    /// Line feed and carriage return are never filtered, even with
    /// every flag set - a paste must keep its line structure.
    #[test]
    fn is_filter_char_never_filters_newline_or_carriage_return() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = TpfGuard::set(u32::MAX);
        assert!(!unsafe { is_filter_char(0x0A) });
        assert!(!unsafe { is_filter_char(0x0D) });
    }

    /// Unnamed C0 controls fall through to the C0 flag, but the
    /// characters with their own flag do NOT - they are matched
    /// earlier.
    #[test]
    fn is_filter_char_uses_c0_only_for_unnamed_controls() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = TpfGuard::set(opt_tpf_flag::C0);

        assert!(unsafe { is_filter_char(0x01) }, "SOH is an unnamed C0");
        assert!(unsafe { is_filter_char(0x1F) }, "US is an unnamed C0");
        // These have their own flags, so C0 alone must not filter them.
        assert!(!unsafe { is_filter_char(0x08) });
        assert!(!unsafe { is_filter_char(0x09) });
        assert!(!unsafe { is_filter_char(0x1b) });
        // ...nor the never-filtered pair.
        assert!(!unsafe { is_filter_char(0x0A) });
    }

    /// The C1 range is 0x80..=0x9F inclusive at both ends, and DEL
    /// (0x7F) sits just below it with its own flag.
    #[test]
    fn is_filter_char_bounds_the_c1_range_exactly() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = TpfGuard::set(opt_tpf_flag::C1);

        assert!(unsafe { is_filter_char(0x80) }, "inclusive lower bound");
        assert!(unsafe { is_filter_char(0x9F) }, "inclusive upper bound");
        assert!(!unsafe { is_filter_char(0x7F) }, "DEL has its own flag");
        assert!(!unsafe { is_filter_char(0xA0) }, "just past the range");
    }

    /// Ordinary printable characters are never filtered.
    #[test]
    fn is_filter_char_never_filters_printable_characters() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = TpfGuard::set(u32::MAX);
        for c in [b' '.into(), b'a'.into(), b'~'.into(), 0x100, 0x20AC] {
            assert!(!unsafe { is_filter_char(c) }, "char {c:#x} must not filter");
        }
    }
}
