//! Translated from `src/nvim/decoration_provider.c` (tractable core
//! only).
//!
//! `decoration_provider.c` implements the Lua-callback-driven extmark
//! decoration-provider hooks (`nvim_set_decoration_provider`) invoked
//! throughout the real screen-rendering pipeline. Every
//! `decor_providers_invoke_*` function needs `decor_provider_invoke`
//! (calls into the Lua host via `nlua_call_ref`), not translated -
//! not a narrow gap, this whole file's real purpose is Lua-callback
//! dispatch during rendering, neither of which exists yet.
//!
//! Translated: `decor_provider_clear`/`decor_free_all_mem` (release a
//! provider's/every provider's Lua refs). `NLUA_CLEAR_REF`'s own
//! `api_free_luaref` call (needs the real Lua host) is
//! `unimplemented!()` if a field is EVER genuinely `!= LUA_NOREF` when
//! this runs - provably unreached today, since nothing translated can
//! construct a `DecorProvider` holding a real (non-`LUA_NOREF`) Lua
//! reference in the first place (`nvim_set_decoration_provider`, the
//! only real way to populate one, isn't translated) - matching the
//! established "translate the real early-return condition, not a
//! hardcoded shortcut" pattern (e.g. `autocmd.rs`'s `AU_NEED_CLEAN`).
//!
//! Deferred: everything else in the file.

use crate::decoration_defs::{DecorProvider, DecorProviderState};
use crate::globals::GlobalCell;
use crate::types_defs::LuaRef;

/// `decor_providers` - every currently-registered decoration
/// provider. Always empty today: nothing translated can register a
/// real one (`nvim_set_decoration_provider`, not translated).
static DECOR_PROVIDERS: GlobalCell<Vec<DecorProvider>> = GlobalCell::new(Vec::new());

/// Clear one Lua-ref field, matching `NLUA_CLEAR_REF`'s own contract:
/// if genuinely holding a real reference, release it via
/// `api_free_luaref` first (not translated - provably unreached
/// today, see this module's own doc comment), then reset to
/// `LUA_NOREF`.
fn clear_lua_ref(field: &mut LuaRef) {
    const LUA_NOREF: LuaRef = -1;
    if *field != LUA_NOREF {
        unimplemented!(
            "clear_lua_ref: releasing a real Lua reference needs \
             api_free_luaref (the Lua host), not translated - \
             unreachable today since nothing can construct a real \
             (non-LUA_NOREF) decoration-provider Lua reference yet"
        );
    }
    *field = LUA_NOREF;
}

/// Disable and clear a decoration provider's own Lua callback
/// references (`decor_provider_clear`). A no-op if `p` is `None`
/// (matches the original's own `if (p == NULL) { return; }` - modeled
/// as `Option<&mut DecorProvider>` rather than a raw nullable
/// pointer, since every real call site already has an ordinary Rust
/// reference on hand).
///
/// `hl_def` is deliberately NOT cleared here, matching the original
/// exactly - its own lifecycle is managed separately (by
/// `decor_provider_invalidate_hl`, not translated).
pub fn decor_provider_clear(p: Option<&mut DecorProvider>) {
    let Some(p) = p else {
        return;
    };
    clear_lua_ref(&mut p.redraw_start);
    clear_lua_ref(&mut p.redraw_buf);
    clear_lua_ref(&mut p.redraw_win);
    clear_lua_ref(&mut p.redraw_line);
    clear_lua_ref(&mut p.redraw_range);
    clear_lua_ref(&mut p.redraw_end);
    clear_lua_ref(&mut p.spell_nav);
    clear_lua_ref(&mut p.conceal_line);
    p.state = DecorProviderState::Disabled;
}

/// Release every registered decoration provider (`decor_free_all_mem`).
///
/// # Safety
/// Single-threaded test/editor state, matching every other
/// `GlobalCell`-backed file-static in this crate.
pub unsafe fn decor_free_all_mem() {
    // SAFETY: forwarded from this function's own safety doc.
    let providers = unsafe { DECOR_PROVIDERS.get_mut() };
    for p in providers.iter_mut() {
        decor_provider_clear(Some(p));
    }
    providers.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::global_state_test_lock;

    const LUA_NOREF: LuaRef = -1;

    #[test]
    fn decor_provider_clear_is_a_noop_for_none() {
        // Must not panic.
        decor_provider_clear(None);
    }

    #[test]
    fn decor_provider_clear_resets_every_ref_and_state() {
        let mut p = DecorProvider::new(0);
        p.state = DecorProviderState::Active;
        // Every field starts at LUA_NOREF already (DecorProvider::new's
        // own init) - this test only exercises the "already cleared"
        // fast path, since nothing can construct a real Lua ref here.
        decor_provider_clear(Some(&mut p));
        assert_eq!(p.state, DecorProviderState::Disabled);
        assert_eq!(p.redraw_start, LUA_NOREF);
        assert_eq!(p.redraw_buf, LUA_NOREF);
        assert_eq!(p.redraw_win, LUA_NOREF);
        assert_eq!(p.redraw_line, LUA_NOREF);
        assert_eq!(p.redraw_range, LUA_NOREF);
        assert_eq!(p.redraw_end, LUA_NOREF);
        assert_eq!(p.spell_nav, LUA_NOREF);
        assert_eq!(p.conceal_line, LUA_NOREF);
    }

    #[test]
    fn decor_provider_clear_leaves_hl_def_untouched() {
        let mut p = DecorProvider::new(0);
        p.hl_def = 42; // a real ref would never exist here today, but
                        // this proves the field genuinely isn't touched.
        decor_provider_clear(Some(&mut p));
        assert_eq!(p.hl_def, 42);
    }

    #[test]
    #[should_panic(expected = "releasing a real Lua reference needs")]
    fn clear_lua_ref_panics_on_a_real_reference() {
        let mut field: LuaRef = 7;
        clear_lua_ref(&mut field);
    }

    #[test]
    fn decor_free_all_mem_clears_every_provider_and_empties_the_list() {
        let _lock = global_state_test_lock();
        {
            let providers = unsafe { DECOR_PROVIDERS.get_mut() };
            providers.clear();
            let mut p1 = DecorProvider::new(1);
            p1.state = DecorProviderState::Active;
            let mut p2 = DecorProvider::new(2);
            p2.state = DecorProviderState::WinDisabled;
            providers.push(p1);
            providers.push(p2);
        }
        unsafe { decor_free_all_mem() };
        let providers = unsafe { DECOR_PROVIDERS.get_mut() };
        assert!(providers.is_empty());
    }
}
