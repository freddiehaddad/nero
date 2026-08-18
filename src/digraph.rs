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
//! Also [`keymap_init`]'s empty-option path, which unloads the active
//! keymap name and clears the initialization flag. Loading a nonempty
//! keymap remains deferred with runtime-file sourcing.
//!
//! Deferred: everything else - `getdigraph`/`digraph_get`/`putdigraph`/
//! `listdigraphs`/`ex_digraphs`/`f_digraph_get(list)`/`ex_loadkeymap`
//! and nonempty keymap loading, all needing the remaining
//! digraph/keymap or runtime-source machinery.

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

/// One `'keymap'` entry: the typed characters and what they map to
/// (`kmap_T`).
///
/// Both fields are owned `char *` in the original; they are owned
/// `Vec<u8>` here, which is what lets [`keymap_ga_clear`] collapse
/// into a single call - see its own doc comment.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KmapT {
    /// the characters that are typed (`from`).
    pub from: Vec<u8>,
    /// what they are mapped to (`to`).
    pub to: Vec<u8>,
}

/// Releases a buffer's `'keymap'` table (`keymap_ga_clear`).
///
/// The original frees each entry's `from`/`to` strings but leaves the
/// array's length alone, so both of its callers follow it immediately
/// with `ga_clear` to release the array itself. Here the strings are
/// owned `Vec`s, so dropping the entries releases them too and the
/// two steps become one.
///
/// Nothing reads the entries between those two calls in the original,
/// so collapsing them changes no observable behaviour.
pub fn keymap_ga_clear(kmap_ga: &mut TypedGarrayT<KmapT>) {
    kmap_ga.ga_clear();
}

/// Set up the key mapping table for a buffer's `'keymap'`
/// (`keymap_init`).
///
/// The empty-option unload path is complete. A nonempty option needs
/// runtime-file sourcing and `:loadkeymap`, and stops exactly there.
///
/// # Safety
/// `buf.b_vars`, when non-null, must point to a live dictionary. A
/// genuinely loaded keymap still needs the mapping-table subsystem.
pub unsafe fn keymap_init(
    buf: &mut crate::buffer_defs::BufT,
) -> Option<&'static [u8]> {
    buf.b_kmap_state &= !crate::buffer_defs::KEYMAP_INIT;
    if buf.b_p_keymap.as_deref().unwrap_or(&[]).is_empty() {
        if buf.b_kmap_state & crate::buffer_defs::KEYMAP_LOADED != 0 {
            unimplemented!("keymap_init: unloading active :lmap entries needs do_map");
        }
        if !buf.b_vars.is_null()
            && let Some(item) = crate::eval::typval::tv_dict_find(
                Some(unsafe { &mut *buf.b_vars }),
                b"keymap_name",
            )
        {
            unsafe {
                crate::eval::typval::tv_dict_item_remove(&mut *buf.b_vars, item);
            }
        }
        None
    } else {
        unimplemented!("keymap_init: loading keymap runtime files needs source_runtime");
    }
}

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

    // ---- keymap_ga_clear ----

    fn kmap(from: &[u8], to: &[u8]) -> KmapT {
        KmapT {
            from: from.to_vec(),
            to: to.to_vec(),
        }
    }

    /// Clearing releases the entries themselves, not just their
    /// strings: the original needs a following `ga_clear` for that,
    /// which owned `Vec`s make unnecessary here.
    #[test]
    fn keymap_ga_clear_empties_the_table() {
        let mut ga = crate::garray_defs::TypedGarrayT::<KmapT>::new(20);
        ga.items.push(kmap(b"ab", b"\xc3\xa4"));
        ga.items.push(kmap(b"cd", b"\xc3\xb6"));
        assert_eq!(ga.ga_len(), 2);

        keymap_ga_clear(&mut ga);

        assert_eq!(ga.ga_len(), 0);
        assert!(ga.is_empty());
    }

    #[test]
    fn keymap_ga_clear_is_a_noop_on_an_empty_table() {
        let mut ga = crate::garray_defs::TypedGarrayT::<KmapT>::new(20);

        keymap_ga_clear(&mut ga);

        assert_eq!(ga.ga_len(), 0);
    }

    /// The table is reusable afterwards, so unloading a keymap and
    /// loading another one works.
    #[test]
    fn keymap_ga_clear_leaves_the_table_reusable() {
        let mut ga = crate::garray_defs::TypedGarrayT::<KmapT>::new(20);
        ga.items.push(kmap(b"ab", b"x"));

        keymap_ga_clear(&mut ga);
        ga.items.push(kmap(b"cd", b"y"));

        assert_eq!(ga.ga_len(), 1);
        assert_eq!(ga.items[0], kmap(b"cd", b"y"));
    }

    #[test]
    fn keymap_init_empty_value_clears_init_and_removes_keymap_name() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        assert!(!dict.is_null());
        assert_eq!(
            crate::eval::typval::tv_dict_add_str(
                unsafe { &mut *dict },
                b"keymap_name",
                Some(b"old"),
            ),
            crate::vim_defs::OK
        );
        let mut buf = crate::buffer_defs::BufT {
            b_vars: dict,
            b_p_keymap: Some(Vec::new()),
            b_kmap_state: crate::buffer_defs::KEYMAP_INIT,
            ..Default::default()
        };

        assert_eq!(unsafe { keymap_init(&mut buf) }, None);
        assert_eq!(buf.b_kmap_state & crate::buffer_defs::KEYMAP_INIT, 0);
        assert!(crate::eval::typval::tv_dict_find(
            Some(unsafe { &mut *dict }),
            b"keymap_name",
        )
        .is_none());

        buf.b_vars = std::ptr::null_mut();
        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    #[should_panic(expected = "source_runtime")]
    fn keymap_init_nonempty_value_needs_runtime_file_sourcing() {
        let mut buf = crate::buffer_defs::BufT {
            b_p_keymap: Some(b"russian-jcukenwin".to_vec()),
            b_kmap_state: crate::buffer_defs::KEYMAP_INIT,
            ..Default::default()
        };
        let _ = unsafe { keymap_init(&mut buf) };
    }

    #[test]
    #[should_panic(expected = "do_map")]
    fn keymap_init_loaded_empty_value_needs_mapping_removal() {
        let mut buf = crate::buffer_defs::BufT {
            b_p_keymap: Some(Vec::new()),
            b_kmap_state: crate::buffer_defs::KEYMAP_LOADED,
            ..Default::default()
        };
        let _ = unsafe { keymap_init(&mut buf) };
    }

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
