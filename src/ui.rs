//! Translated from `src/nvim/ui.c` (a single harvested function only).
//!
//! `ui.c` implements the whole UI-client protocol (grid updates,
//! highlight state, the msgpack-RPC `nvim_ui_attach` handshake, etc.),
//! none of which is translated. Harvested one small, self-contained
//! piece ahead of the rest of the file: `ui_has` (needed by
//! `window.c`'s `tabline_height`), matching this crate's established
//! "one tractable function ahead of a huge file" precedent
//! (`ex_docmd.rs`, `search.rs`, etc.).
//!
//! `ui_has` reads the original's own file-static `bool ui_ext
//! [kUIExtCount] = { 0 }` array - only ever mutated by `ui_refresh`/
//! the real UI-attachment negotiation machinery (`nvim_ui_attach`),
//! none of which is translated. Since nothing in this crate can
//! currently attach a real UI, that array genuinely stays all-`false`
//! forever in any session this crate can construct today - `ui_has`
//! is translated as an always-`false` predicate, matching this
//! crate's established "always-empty-registry" precedent
//! (`crate::autocmd::AUTOCMDS`) rather than modeling the full,
//! currently-inert array.

/// UI extension/capability identifiers (`UIExtension`), mechanically
/// transcribed from `ui_defs.h` for [`ui_has`]'s own parameter type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum UiExtension {
    Cmdline = 0,
    Popupmenu = 1,
    Tabline = 2,
    Wildmenu = 3,
    Messages = 4,
    Linegrid = 5,
    Multigrid = 6,
    HlState = 7,
    TermColors = 8,
    FloatDebug = 9,
}

/// Returns `true` if the given UI extension is enabled (`ui_has`).
///
/// Always `false` - see this module's own doc comment.
#[must_use]
pub const fn ui_has(_ext: UiExtension) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_has_is_always_false() {
        assert!(!ui_has(UiExtension::Tabline));
        assert!(!ui_has(UiExtension::Cmdline));
        assert!(!ui_has(UiExtension::FloatDebug));
    }
}
