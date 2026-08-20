//! Translated from `src/nvim/eval/gc.c` in full.

use crate::eval::typval_defs::{DictT, ListT};
use crate::globals::GlobalCell;

/// Head of the linked list of all live dictionaries
/// (`gc_first_dict`).
pub(crate) static GC_FIRST_DICT: GlobalCell<*mut DictT> =
    GlobalCell::new(std::ptr::null_mut());

/// Head of the linked list of all live lists (`gc_first_list`).
pub(crate) static GC_FIRST_LIST: GlobalCell<*mut ListT> =
    GlobalCell::new(std::ptr::null_mut());
