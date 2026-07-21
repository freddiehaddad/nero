//! Translated from `src/nvim/statusline_defs.h` (partial).
//!
//! `StlFlag`, `StlClickDefinition`, `StcClick`, `StlClickRecord`,
//! `StlHlrecT`, `statuscol_T` are translated here. `stl_item_t` is used
//! only by `statusline.c` itself (phase 8) and isn't needed before then.

use crate::fold_defs::FoldinfoT;
use crate::pos_defs::{ColnrT, LinenrT};
use crate::sign_defs::SignTextAttrs;

/// `'statusline'` item flags (`StlFlag`). Kept as their literal ASCII byte
/// values (matching the original's char-literal enum) via `u8` consts
/// rather than a Rust `enum`, since the "item flag" concept is directly
/// used as a `char` byte elsewhere (e.g. `StlHlRec.item`) - a Rust enum
/// would need explicit `as u8`/repr gymnastics for zero benefit here.
pub mod stl_flag {
    /// Path of file in buffer.
    pub const FILEPATH: u8 = b'f';
    /// Full path of file in buffer.
    pub const FULLPATH: u8 = b'F';
    /// Last part (tail) of file path.
    pub const FILENAME: u8 = b't';
    /// Column of cursor.
    pub const COLUMN: u8 = b'c';
    /// Virtual column.
    pub const VIRTCOL: u8 = b'v';
    /// - with 'if different' display.
    pub const VIRTCOL_ALT: u8 = b'V';
    /// Line number of cursor.
    pub const LINE: u8 = b'l';
    /// Number of lines in buffer.
    pub const NUMLINES: u8 = b'L';
    /// Current buffer number.
    pub const BUFNO: u8 = b'n';
    /// `'keymap'` when active.
    pub const KEYMAP: u8 = b'k';
    /// Offset of character under cursor.
    pub const OFFSET: u8 = b'o';
    /// - in hexadecimal.
    pub const OFFSET_X: u8 = b'O';
    /// Byte value of character.
    pub const BYTEVAL: u8 = b'b';
    /// - in hexadecimal.
    pub const BYTEVAL_X: u8 = b'B';
    /// Readonly flag.
    pub const ROFLAG: u8 = b'r';
    /// - other display.
    pub const ROFLAG_ALT: u8 = b'R';
    /// Window is showing a help file.
    pub const HELPFLAG: u8 = b'h';
    /// - other display.
    pub const HELPFLAG_ALT: u8 = b'H';
    /// `'filetype'`.
    pub const FILETYPE: u8 = b'y';
    /// - other display.
    pub const FILETYPE_ALT: u8 = b'Y';
    /// Window is showing the preview buf.
    pub const PREVIEWFLAG: u8 = b'w';
    /// - other display.
    pub const PREVIEWFLAG_ALT: u8 = b'W';
    /// Modified flag.
    pub const MODIFIED: u8 = b'm';
    /// - other display.
    pub const MODIFIED_ALT: u8 = b'M';
    /// Quickfix window description.
    pub const QUICKFIX: u8 = b'q';
    /// Percentage through file.
    pub const PERCENTAGE: u8 = b'p';
    /// Percentage as TOP BOT ALL or NN%.
    pub const ALTPERCENT: u8 = b'P';
    /// Argument list status as (x of y).
    pub const ARGLISTSTAT: u8 = b'a';
    /// Page number (when printing).
    pub const PAGENUM: u8 = b'N';
    /// `'showcmd'` buffer
    pub const SHOWCMD: u8 = b'S';
    /// Fold column for `'statuscolumn'`
    pub const FOLDCOL: u8 = b'C';
    /// Sign column for `'statuscolumn'`
    pub const SIGNCOL: u8 = b's';
    /// Start of expression to substitute.
    pub const VIM_EXPR: u8 = b'{';
    /// Separation between alignment sections.
    pub const SEPARATE: u8 = b'=';
    /// Truncation mark if line is too long.
    pub const TRUNCMARK: u8 = b'<';
    /// Highlight from (User)1..9 or 0.
    pub const USER_HL: u8 = b'*';
    /// Highlight name.
    pub const HIGHLIGHT: u8 = b'#';
    /// Highlight name (combining previous attrs).
    pub const HIGHLIGHT_COMB: u8 = b'$';
    /// Tab page label nr.
    pub const TABPAGENR: u8 = b'T';
    /// Tab page close nr.
    pub const TABCLOSENR: u8 = b'X';
    /// Click region start.
    pub const CLICK_FUNC: u8 = b'@';
}

/// Type of a status-line click (`the anonymous enum inside StlClickDefinition`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StlClickType {
    /// Clicks to this area are ignored.
    #[default]
    Disabled = 0,
    /// Switch to the given tab.
    TabSwitch,
    /// Close given tab.
    TabClose,
    /// Run user function.
    FuncRun,
}

/// Status line click definition (`StlClickDefinition`).
#[derive(Debug, Clone, Default)]
pub struct StlClickDefinition {
    /// Type of the click.
    pub r#type: StlClickType,
    /// Tab page number.
    pub tabnr: i32,
    /// Function to run.
    pub func: Option<Vec<u8>>,
}

/// `StcClick`: an array of click definitions. The original's raw
/// `StlClickDefinition *def; size_t size;` pair becomes a single owned
/// `Vec` (same reasoning as `StringBuilder`/`GarrayT.ga_data` elsewhere in
/// this crate: an owned, growable, explicitly-sized buffer is exactly what
/// `Vec` natively is).
#[derive(Debug, Clone, Default)]
pub struct StcClick {
    /// Click definition(s).
    pub def: Vec<StlClickDefinition>,
}

/// Used for tabline clicks (`StlClickRecord`).
#[derive(Debug, Clone)]
pub struct StlClickRecord {
    /// Click definition.
    pub def: StlClickDefinition,
    /// Byte offset where region starts (in place of the original's raw
    /// `const char *start` pointer into the tabline buffer).
    pub start: usize,
}

/// Used for highlighting in the status line (`stl_hlrec_t` / `struct
/// stl_hlrec`).
#[derive(Debug, Clone, Copy, Default)]
pub struct StlHlrecT {
    /// Byte offset where the item starts in the status line output buffer
    /// (in place of the original's raw `char *start` pointer).
    pub start: usize,
    /// 0: no HL, 1-9: User HL, < 0 for syn ID
    pub userhl: i32,
    /// Item flag belonging to highlight (used for `'statuscolumn'`)
    pub item: u8,
}

/// Struct to hold info for `'statuscolumn'` (`statuscol_T`).
#[derive(Debug, Clone, Default)]
pub struct StatuscolT {
    /// width of the status column
    pub width: i32,
    /// buffer line being drawn
    pub lnum: LinenrT,
    /// cursorline sign highlight id
    pub sign_cul_id: i32,
    /// whether to draw the statuscolumn
    pub draw: bool,
    /// highlight groups (in place of the original's raw `stl_hlrec_t *`
    /// pointer to a heap-allocated array).
    pub hlrec: Vec<StlHlrecT>,
    /// fold information
    pub foldinfo: FoldinfoT,
    /// vcol array filled for fold item
    pub fold_vcol: [ColnrT; 9],
    /// sign attributes (in place of the original's raw `SignTextAttrs *`
    /// pointer to a heap-allocated array, max `SIGN_SHOW_MAX` entries).
    pub sattrs: Vec<SignTextAttrs>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuscol_default_has_no_signs_or_highlights() {
        let sc = StatuscolT::default();
        assert!(sc.hlrec.is_empty());
        assert!(sc.sattrs.is_empty());
        assert!(!sc.draw);
    }

    #[test]
    fn stl_click_type_default_is_disabled() {
        // Disabled = 0: clicks to this area are ignored by default.
        assert_eq!(StlClickType::default(), StlClickType::Disabled);
    }
}
