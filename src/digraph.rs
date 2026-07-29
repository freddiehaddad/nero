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
