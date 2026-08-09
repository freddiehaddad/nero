//! Translated from `src/nvim/digraph.c` (tractable core only).
//!
//! `digraph.c` (~2100 lines) manages digraph (`i_CTRL-K`) character
//! composition - almost entirely dependent on the large built-in and
//! user-defined digraph tables (`digraphdefault`/`user_digraphs`),
//! neither translated.
//!
//! Translated: [`check_digraph_chars_valid`] - a small, pure validation
//! function with no dependency on the digraph tables themselves.
//!
//! Deferred: everything else - `getdigraph`/`digraph_get`/`putdigraph`/
//! `listdigraphs`/`ex_digraphs`/`f_digraph_get(list)`/`ex_loadkeymap`
//! and the keymap-loading machinery, all needing the digraph/keymap
//! table storage.

use crate::ascii_defs::ESC;
use crate::garray_defs::TypedGarrayT;
use crate::globals::GlobalCell;

/// The character a digraph composes to (`result_T`).
pub type ResultT = i32;

/// One digraph: two input characters and the character they compose
/// to (`digr_T`).
///
/// `char1`/`char2` are `uint8_t` in the original, not `int`, so a
/// digraph's own two input characters are single bytes even though
/// every function taking them uses `int`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DigrT {
    /// first character of the digraph (`char1`).
    pub char1: u8,
    /// second character of the digraph (`char2`).
    pub char2: u8,
    /// the character the pair composes to (`result`).
    pub result: ResultT,
}

/// `user_digraphs` - digraphs defined by `:digraphs`, searched before
/// the built-in table.
///
/// A [`TypedGarrayT`] rather than the erased `GarrayT`, matching this
/// crate's established treatment of the original's other growarrays of
/// a struct; `ga_len` is derived from the `Vec` rather than stored.
pub static USER_DIGRAPHS: GlobalCell<TypedGarrayT<DigrT>> =
    GlobalCell::new(TypedGarrayT::new(10));

/// Add a digraph to the user table, or update the one already there
/// (`registerdigraph`).
///
/// The original stores `char1`/`char2` as `uint8_t` while taking them
/// as `int`, so a value above 255 is truncated. That narrowing is
/// preserved rather than guarded against: `putdigraph`, the only real
/// caller, validates its input through
/// [`check_digraph_chars_valid`] first.
///
/// # Safety
/// Touches the [`USER_DIGRAPHS`] file-static.
pub unsafe fn registerdigraph(char1: i32, char2: i32, n: ResultT) {
    // SAFETY: forwarded from this function's own safety doc.
    let table = unsafe { USER_DIGRAPHS.get_mut() };

    // If the digraph already exists, replace "result".
    for dp in &mut table.items {
        if i32::from(dp.char1) == char1 && i32::from(dp.char2) == char2 {
            dp.result = n;
            return;
        }
    }

    // Add a new digraph to the table.
    table.items.push(DigrT {
        char1: char1 as u8,
        char2: char2 as u8,
        result: n,
    });
}

/// Whether `char1`/`char2` are valid as a digraph's own two input
/// characters (`check_digraph_chars_valid`). `char2 == 0` means only one
/// character was given (invalid - a digraph always takes exactly two);
/// `ESC` is never allowed in either position.
///
/// Omits the original's own `semsg`/`emsg` message display, matching
/// this crate's established "skip the deferred message-display side
/// effect, keep the exact same return value" policy (e.g.
/// `window::check_split_disallowed_err`).
#[must_use]
pub fn check_digraph_chars_valid(char1: i32, char2: i32) -> bool {
    if char2 == 0 {
        return false;
    }
    if char1 == i32::from(ESC) || char2 == i32::from(ESC) {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- registerdigraph ----

    /// Empties the user digraph table before the test and restores it
    /// afterwards, even through a panic.
    struct DigraphGuard {
        saved: Vec<DigrT>,
    }

    impl DigraphGuard {
        fn empty() -> Self {
            let table = unsafe { USER_DIGRAPHS.get_mut() };
            let saved = std::mem::take(&mut table.items);
            Self { saved }
        }

        fn items() -> Vec<DigrT> {
            unsafe { USER_DIGRAPHS.get_mut() }.items.clone()
        }
    }

    impl Drop for DigraphGuard {
        fn drop(&mut self) {
            unsafe { USER_DIGRAPHS.get_mut() }.items = std::mem::take(&mut self.saved);
        }
    }

    #[test]
    fn registerdigraph_appends_a_new_digraph() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = DigraphGuard::empty();

        unsafe { registerdigraph('a' as i32, 'b' as i32, 0x2018) };
        assert_eq!(
            DigraphGuard::items(),
            vec![DigrT { char1: b'a', char2: b'b', result: 0x2018 }]
        );
    }

    /// Registering an existing pair REPLACES its result rather than
    /// appending a second entry.
    #[test]
    fn registerdigraph_replaces_the_result_of_an_existing_pair() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = DigraphGuard::empty();

        unsafe { registerdigraph('a' as i32, 'b' as i32, 1) };
        unsafe { registerdigraph('c' as i32, 'd' as i32, 2) };
        unsafe { registerdigraph('a' as i32, 'b' as i32, 99) };

        assert_eq!(
            DigraphGuard::items(),
            vec![
                DigrT { char1: b'a', char2: b'b', result: 99 },
                DigrT { char1: b'c', char2: b'd', result: 2 },
            ],
            "the pair is updated in place, not appended again"
        );
    }

    /// Both characters must match for a replacement; sharing only one
    /// makes a distinct digraph.
    #[test]
    fn registerdigraph_treats_a_shared_first_character_as_distinct() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = DigraphGuard::empty();

        unsafe { registerdigraph('a' as i32, 'b' as i32, 1) };
        unsafe { registerdigraph('a' as i32, 'c' as i32, 2) };
        unsafe { registerdigraph('x' as i32, 'b' as i32, 3) };

        assert_eq!(DigraphGuard::items().len(), 3);
    }

    /// The pair is ordered: "ab" and "ba" are different digraphs.
    #[test]
    fn registerdigraph_distinguishes_the_order_of_the_pair() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = DigraphGuard::empty();

        unsafe { registerdigraph('a' as i32, 'b' as i32, 1) };
        unsafe { registerdigraph('b' as i32, 'a' as i32, 2) };

        assert_eq!(
            DigraphGuard::items(),
            vec![
                DigrT { char1: b'a', char2: b'b', result: 1 },
                DigrT { char1: b'b', char2: b'a', result: 2 },
            ]
        );
    }

    /// The result may be any character value, including one well past
    /// a byte - only the two INPUT characters are bytes.
    #[test]
    fn registerdigraph_keeps_a_wide_result_intact() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = DigraphGuard::empty();

        unsafe { registerdigraph('S' as i32, 'E' as i32, 0x1F600) };
        assert_eq!(DigraphGuard::items()[0].result, 0x1F600);
    }

    #[test]
    fn rejects_a_single_character() {
        assert!(!check_digraph_chars_valid('a' as i32, 0));
    }

    #[test]
    fn rejects_escape_in_either_position() {
        assert!(!check_digraph_chars_valid(i32::from(ESC), 'a' as i32));
        assert!(!check_digraph_chars_valid('a' as i32, i32::from(ESC)));
    }

    #[test]
    fn accepts_two_ordinary_characters() {
        assert!(check_digraph_chars_valid('a' as i32, 'e' as i32));
        assert!(check_digraph_chars_valid('/' as i32, '/' as i32));
    }

    #[test]
    fn accepts_a_non_ascii_codepoint() {
        // char1/char2 are full Unicode codepoints, not just bytes.
        assert!(check_digraph_chars_valid('é' as i32, 'e' as i32));
    }
}
