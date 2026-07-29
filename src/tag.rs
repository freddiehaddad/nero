//! Translated from `src/nvim/tag.c` (tractable core only).
//!
//! `tag.c` (~3900 lines) is the `:tag`/tag-stack/tags-file navigation
//! subsystem - almost all of it needs the tags-file search engine
//! (`find_tags`), `:tag`/`:pop`/`:tselect` Ex-command handling, and
//! real buffer switching/window management, none of which are
//! translated yet. Translated: the read-only tag-stack introspection
//! needed by the `gettagstack()` builtin (`get_tag_details`/
//! [`get_tagstack`]), plus [`tag_strnicmp`] (a small, pure,
//! case-insensitive comparator used for tag sorting).
//!
//! `tag.c`'s own `tagstack_clear_entry` was already translated in an
//! earlier session, hosted in `mark.rs` alongside its only real
//! consumer (`mark_forget_file`) rather than waiting for the rest of
//! this file - see that module's own doc comment.

use crate::buffer_defs::{TaggyT, WinT};

/// Add the details of a single tag-stack entry to `retdict`
/// (`get_tag_details`).
fn get_tag_details(tag: &TaggyT, retdict: &mut crate::eval::typval_defs::DictT) {
    use crate::eval::typval::{tv_dict_add_list, tv_dict_add_nr, tv_dict_add_str, tv_list_alloc, tv_list_append_number};
    use crate::pos_defs::MAXCOL;

    tv_dict_add_str(retdict, b"tagname", Some(&tag.tagname));
    tv_dict_add_nr(retdict, b"matchnr", i64::from(tag.cur_match) + 1);
    tv_dict_add_nr(retdict, b"bufnr", i64::from(tag.cur_fnum));
    if let Some(user_data) = &tag.user_data {
        tv_dict_add_str(retdict, b"user_data", Some(user_data));
    }

    let pos = tv_list_alloc(4);
    // SAFETY: `pos` was just allocated above, not yet shared beyond
    // `retdict` (which only holds a refcounted reference via
    // `tv_dict_add_list`).
    unsafe {
        tv_dict_add_list(retdict, b"from", pos);

        let fmark = &tag.fmark;
        tv_list_append_number(pos, if fmark.fnum != -1 { i64::from(fmark.fnum) } else { 0 });
        tv_list_append_number(pos, i64::from(fmark.mark.lnum));
        tv_list_append_number(pos, i64::from(if fmark.mark.col == MAXCOL { MAXCOL } else { fmark.mark.col + 1 }));
        tv_list_append_number(pos, i64::from(fmark.mark.coladd));
    }
}

/// Return the tag stack entries of window `wp` in dictionary
/// `retdict` (`get_tagstack`).
///
/// # Safety
/// `retdict` must be a valid, non-null pointer to a freshly-allocated,
/// not-yet-shared `DictT` (matching [`crate::eval::typval::tv_dict_alloc_ret`]'s
/// own contract).
pub unsafe fn get_tagstack(wp: &WinT, retdict: *mut crate::eval::typval_defs::DictT) {
    use crate::eval::typval::{tv_dict_add_list, tv_dict_add_nr, tv_list_alloc, tv_list_append_dict};

    // SAFETY: forwarded from this function's own safety doc.
    let retdict_ref = unsafe { &mut *retdict };
    tv_dict_add_nr(retdict_ref, b"length", i64::from(wp.w_tagstacklen));
    tv_dict_add_nr(retdict_ref, b"curidx", i64::from(wp.w_tagstackidx) + 1);
    let l = tv_list_alloc(2);
    // SAFETY: `l` was just allocated above, not yet shared beyond
    // `retdict` (which only holds a refcounted reference).
    unsafe { tv_dict_add_list(retdict_ref, b"items", l) };

    for i in 0..wp.w_tagstacklen as usize {
        let d = crate::eval::typval::tv_dict_alloc();
        // SAFETY: `d`/`l` are both valid, freshly-obtained live
        // pointers (`l` just allocated above; `tv_dict_alloc` never
        // returns null).
        unsafe { tv_list_append_dict(l, d) };
        // SAFETY: `d` was just returned by `tv_dict_alloc` above, not
        // yet shared beyond `l`.
        get_tag_details(&wp.w_tagstack[i], unsafe { &mut *d });
    }
}

/// Compares `s1`/`s2` for `len` bytes, ignoring ASCII case (folding to
/// uppercase, matching `'sort -f'`) - `0` for a match, negative if
/// `s1` sorts first, positive if `s2` sorts first (`tag_strnicmp`).
///
/// A byte beyond either slice's own length (when the other is longer)
/// is treated as `0`/NUL, matching how a shorter, real NUL-terminated
/// C string would naturally compare here.
#[must_use]
pub fn tag_strnicmp(s1: &[u8], s2: &[u8], len: usize) -> i32 {
    for k in 0..len {
        let c1 = s1.get(k).copied().unwrap_or(0);
        let c2 = s2.get(k).copied().unwrap_or(0);
        let diff =
            crate::macros_defs::toupper_asc(i32::from(c1)) - crate::macros_defs::toupper_asc(i32::from(c2));
        if diff != 0 {
            return diff;
        }
        if c1 == 0 {
            break;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::typval_defs::{DictitemT, TypvalT, TypvalValue};

    fn dict_get<'a>(d: &'a crate::eval::typval_defs::DictT, key: &[u8]) -> Option<&'a TypvalT> {
        // SAFETY: `d.dv_index` only ever holds still-live `DictitemT`
        // pointers owned by `d` itself.
        unsafe { d.dv_index.values().map(|&p| &*p).find(|item: &&DictitemT| item.di_key.starts_with(key)) }
            .map(|item| &item.di_tv)
    }

    fn list_items(l: *mut crate::eval::typval_defs::ListT) -> Vec<TypvalT> {
        let mut out = Vec::new();
        // SAFETY: `l` is a valid, live list pointer for the whole
        // duration of this walk.
        unsafe {
            let mut item = (*l).lv_first;
            while !item.is_null() {
                out.push((*item).li_tv.clone());
                item = (*item).li_next;
            }
        }
        out
    }

    // --- get_tag_details ---

    #[test]
    fn get_tag_details_populates_all_fields() {
        let _lock = crate::globals::global_state_test_lock();
        let tag = TaggyT {
            tagname: b"my_func".to_vec(),
            fmark: crate::mark_defs::FmarkT {
                mark: crate::pos_defs::PosT { lnum: 12, col: 3, coladd: 0 },
                fnum: 5,
                ..Default::default()
            },
            cur_match: 1,
            cur_fnum: 5,
            user_data: Some(b"udata".to_vec()),
        };

        let d = crate::eval::typval::tv_dict_alloc();
        // SAFETY: `d` was just allocated, exclusively owned here.
        get_tag_details(&tag, unsafe { &mut *d });

        // SAFETY: `d` is still a valid, exclusively-held pointer.
        let d_ref = unsafe { &*d };
        assert_eq!(dict_get(d_ref, b"tagname").unwrap().value, TypvalValue::String(Some(b"my_func".to_vec())));
        assert_eq!(dict_get(d_ref, b"matchnr").unwrap().value, TypvalValue::Number(2));
        assert_eq!(dict_get(d_ref, b"bufnr").unwrap().value, TypvalValue::Number(5));
        assert_eq!(dict_get(d_ref, b"user_data").unwrap().value, TypvalValue::String(Some(b"udata".to_vec())));

        let TypvalValue::List(from) = dict_get(d_ref, b"from").unwrap().value else { panic!("expected a List") };
        let items = list_items(from);
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].value, TypvalValue::Number(5));
        assert_eq!(items[1].value, TypvalValue::Number(12));
        assert_eq!(items[2].value, TypvalValue::Number(4)); // col + 1
        assert_eq!(items[3].value, TypvalValue::Number(0));

        // SAFETY: `d` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_dict_free(d) };
    }

    #[test]
    fn get_tag_details_no_user_data_key_when_none() {
        let _lock = crate::globals::global_state_test_lock();
        let tag = TaggyT { tagname: b"f".to_vec(), user_data: None, ..Default::default() };

        let d = crate::eval::typval::tv_dict_alloc();
        // SAFETY: `d` was just allocated, exclusively owned here.
        get_tag_details(&tag, unsafe { &mut *d });

        // SAFETY: `d` is still a valid, exclusively-held pointer.
        assert!(dict_get(unsafe { &*d }, b"user_data").is_none());

        // SAFETY: `d` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_dict_free(d) };
    }

    #[test]
    fn get_tag_details_maxcol_column_stays_maxcol() {
        let _lock = crate::globals::global_state_test_lock();
        let tag = TaggyT {
            tagname: b"f".to_vec(),
            fmark: crate::mark_defs::FmarkT {
                mark: crate::pos_defs::PosT { lnum: 1, col: crate::pos_defs::MAXCOL, coladd: 0 },
                fnum: -1,
                ..Default::default()
            },
            ..Default::default()
        };

        let d = crate::eval::typval::tv_dict_alloc();
        // SAFETY: `d` was just allocated, exclusively owned here.
        get_tag_details(&tag, unsafe { &mut *d });

        // SAFETY: `d` is still a valid, exclusively-held pointer.
        let d_ref = unsafe { &*d };
        let TypvalValue::List(from) = dict_get(d_ref, b"from").unwrap().value else { panic!("expected a List") };
        let items = list_items(from);
        assert_eq!(items[0].value, TypvalValue::Number(0), "fnum == -1 becomes 0");
        assert_eq!(items[2].value, TypvalValue::Number(i64::from(crate::pos_defs::MAXCOL)));

        // SAFETY: `d` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_dict_free(d) };
    }

    // --- get_tagstack ---

    #[test]
    fn get_tagstack_empty_stack() {
        let _lock = crate::globals::global_state_test_lock();
        let win = WinT::default();

        let mut rettv = TypvalT::default();
        // SAFETY: `rettv` is freshly default-initialized.
        let retdict = unsafe { crate::eval::typval::tv_dict_alloc_ret(&mut rettv) };
        // SAFETY: `retdict` was just freshly allocated by
        // `tv_dict_alloc_ret` above.
        unsafe { get_tagstack(&win, retdict) };

        // SAFETY: `retdict` is still a valid, exclusively-held pointer.
        let d_ref = unsafe { &*retdict };
        assert_eq!(dict_get(d_ref, b"length").unwrap().value, TypvalValue::Number(0));
        assert_eq!(dict_get(d_ref, b"curidx").unwrap().value, TypvalValue::Number(1));
        let TypvalValue::List(items) = dict_get(d_ref, b"items").unwrap().value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(items) }, 0);

        // SAFETY: `rettv` owns its own dict exclusively; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_clear_simple(&rettv) };
    }

    #[test]
    fn get_tagstack_reports_entries_and_curidx() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        win.w_tagstack[0] = TaggyT {
            tagname: b"foo".to_vec(),
            fmark: crate::mark_defs::FmarkT {
                mark: crate::pos_defs::PosT { lnum: 3, col: 0, coladd: 0 },
                fnum: 1,
                ..Default::default()
            },
            cur_match: 0,
            cur_fnum: 1,
            user_data: None,
        };
        win.w_tagstack[1] = TaggyT {
            tagname: b"bar".to_vec(),
            fmark: crate::mark_defs::FmarkT {
                mark: crate::pos_defs::PosT { lnum: 7, col: 0, coladd: 0 },
                fnum: 1,
                ..Default::default()
            },
            cur_match: 2,
            cur_fnum: 1,
            user_data: None,
        };
        win.w_tagstacklen = 2;
        win.w_tagstackidx = 1;

        let mut rettv = TypvalT::default();
        // SAFETY: `rettv` is freshly default-initialized.
        let retdict = unsafe { crate::eval::typval::tv_dict_alloc_ret(&mut rettv) };
        // SAFETY: `retdict` was just freshly allocated above.
        unsafe { get_tagstack(&win, retdict) };

        // SAFETY: `retdict` is still a valid, exclusively-held pointer.
        let d_ref = unsafe { &*retdict };
        assert_eq!(dict_get(d_ref, b"length").unwrap().value, TypvalValue::Number(2));
        assert_eq!(dict_get(d_ref, b"curidx").unwrap().value, TypvalValue::Number(2));
        let TypvalValue::List(items) = dict_get(d_ref, b"items").unwrap().value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(items) }, 2);

        let entries = list_items(items);
        let TypvalValue::Dict(d0) = entries[0].value else { panic!("expected a Dict") };
        // SAFETY: `d0` is a still-live dict owned by `items`.
        assert_eq!(dict_get(unsafe { &*d0 }, b"tagname").unwrap().value, TypvalValue::String(Some(b"foo".to_vec())));

        // SAFETY: `rettv` owns its own dict exclusively; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_clear_simple(&rettv) };
    }

    #[test]
    fn tag_strnicmp_equal_strings() {
        assert_eq!(tag_strnicmp(b"Foo", b"foo", 3), 0);
    }

    #[test]
    fn tag_strnicmp_respects_len_bound() {
        // Only the first 3 bytes ("foo") are compared.
        assert_eq!(tag_strnicmp(b"foobar", b"foobaz", 3), 0);
    }

    #[test]
    fn tag_strnicmp_case_insensitive_difference() {
        assert!(tag_strnicmp(b"abc", b"abd", 3) < 0);
        assert!(tag_strnicmp(b"abd", b"abc", 3) > 0);
    }

    #[test]
    fn tag_strnicmp_shorter_string_compares_as_smaller() {
        // "ab" (hits its own end early) vs "abc": comparing 3 bytes,
        // the 3rd position is 0 (implicit NUL) vs 'c'.
        assert!(tag_strnicmp(b"ab", b"abc", 3) < 0);
        assert!(tag_strnicmp(b"abc", b"ab", 3) > 0);
    }

    #[test]
    fn tag_strnicmp_stops_at_embedded_nul_equivalent() {
        // Both empty: comparing for a len longer than either produces
        // 0 immediately (both bytes default to 0 at every position).
        assert_eq!(tag_strnicmp(b"", b"", 5), 0);
    }
}
