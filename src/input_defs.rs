//! Translated from `src/nvim/input_defs.h`.

use crate::api::private::defs::NvimString;

/// Structure used to store one block of the stuff/redo/recording buffers
/// (`struct buffblock`, typedef'd as `buffblock_T`).
///
/// The original's intrusive singly-linked list (`b_next`) plus a C99
/// flexible array member (`char b_str[1]; // contents (actually
/// longer)`) become a `Vec<BuffblockT>`-friendly owned buffer: `b_next`
/// is dropped entirely (the owning [`BuffheaderT`] uses a real `Vec`
/// instead of a manually-linked block list) and `b_str` becomes an owned
/// `Vec<u8>` (`b_strlen` is dropped too, redundant with `Vec::len()`,
/// same reasoning as `UEntry.ue_array`/`ue_size` in `undo_defs.rs`).
#[derive(Debug, Clone, Default)]
pub struct BuffblockT {
    /// contents
    pub b_str: Vec<u8>,
}

/// Header used for the stuff buffer and the redo buffer (`buffheader_T`).
///
/// The original's manually-managed linked list of [`BuffblockT`]s (a
/// dummy first block `bh_first` plus a `bh_curr` pointer into the list,
/// with `bh_space` tracking remaining capacity in the current block for
/// in-place appending) becomes a plain `Vec<BuffblockT>`: appending a new
/// block is just `Vec::push`, and there is no fixed per-block capacity to
/// track, so `bh_space`/`bh_create_newblock` are dropped as an
/// implementation detail of the original's manual block-growing scheme
/// that a growable `Vec` doesn't need.
#[derive(Debug, Clone, Default)]
pub struct BuffheaderT {
    /// blocks of buffered input, in append order
    pub blocks: Vec<BuffblockT>,
    /// index for reading
    pub bh_index: usize,
}

/// `save_redo_T`.
#[derive(Debug, Clone, Default)]
pub struct SaveRedoT {
    pub sr_redobuff: BuffheaderT,
    pub sr_old_redobuff: BuffheaderT,
}

/// Used for the typeahead buffer: `typebuf` (`typebuf_T`).
///
/// `tb_buf`/`tb_noremap` (parallel arrays: buffered characters and their
/// per-character mapping flags) become owned `Vec<u8>`s; `tb_buflen` (the
/// allocated capacity) is dropped as redundant with `Vec::len()`/
/// `Vec::capacity()`.
#[derive(Debug, Clone, Default)]
pub struct TypebufT {
    /// buffer for typed characters
    pub tb_buf: Vec<u8>,
    /// mapping flags for characters in `tb_buf[]`
    pub tb_noremap: Vec<u8>,
    /// current position in `tb_buf[]`
    pub tb_off: i32,
    /// number of valid bytes in `tb_buf[]`
    pub tb_len: i32,
    /// nr of mapped bytes in `tb_buf[]`
    pub tb_maplen: i32,
    /// nr of silently mapped bytes in `tb_buf[]`
    pub tb_silent: i32,
    /// nr of bytes without abbrev. in `tb_buf[]`
    pub tb_no_abbr_cnt: i32,
    /// nr of times `tb_buf` was changed; never zero
    pub tb_change_cnt: i32,
}

/// Struct to hold the saved typeahead for `save_typeahead()` (`tasave_T`).
#[derive(Debug, Clone, Default)]
pub struct TasaveT {
    pub save_typebuf: TypebufT,
    /// true when `save_typebuf` valid
    pub typebuf_valid: bool,
    pub old_char: i32,
    pub old_mod_mask: i32,
    pub save_readbuf1: BuffheaderT,
    pub save_readbuf2: BuffheaderT,
    pub save_inputbuf: NvimString,
}

/// Values for the "noremap" argument of `ins_typebuf()` (not yet
/// translated). Also used for `map->m_noremap` and `menu->noremap[]`
/// (`enum RemapValues`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RemapValues {
    /// Allow remapping.
    #[default]
    Yes = 0,
    /// No remapping.
    None = -1,
    /// Remap script-local mappings only.
    Script = -2,
    /// No remapping for first char.
    Skip = -3,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffheader_default_is_empty() {
        let bh = BuffheaderT::default();
        assert!(bh.blocks.is_empty());
        assert_eq!(bh.bh_index, 0);
    }

    #[test]
    fn typebuf_default_is_zeroed() {
        let tb = TypebufT::default();
        assert!(tb.tb_buf.is_empty());
        assert_eq!(tb.tb_change_cnt, 0);
    }

    #[test]
    fn remap_values_match_c_enum() {
        assert_eq!(RemapValues::Yes as i32, 0);
        assert_eq!(RemapValues::None as i32, -1);
        assert_eq!(RemapValues::Script as i32, -2);
        assert_eq!(RemapValues::Skip as i32, -3);
        assert_eq!(RemapValues::default(), RemapValues::Yes);
    }
}
