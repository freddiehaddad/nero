//! Window-related Vimscript helpers from `src/nvim/eval/window.c`.
//!
//! These helpers predate this file and remain implemented in
//! `crate::window`; re-export them here so the Rust module tree mirrors
//! their actual Neovim source file without duplicating behavior.

pub use crate::window::{
    find_tabwin, find_win_by_nr, find_win_by_nr_or_id, get_winnr,
    win_findbuf, win_getid, win_has_winnr, win_id2win, win_id2wp,
    win_id2wp_tp,
};
