//! Translated from `src/nvim/ui_compositor.c` (initial state helpers).

/// Number of UIs using internal composition (`composed_uis`).
static COMPOSED_UIS: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);
/// Whether the composed screen is currently valid (`valid_screen`).
static VALID_SCREEN: crate::globals::GlobalCell<bool> =
    crate::globals::GlobalCell::new(true);
/// Scratch composed glyph line (`linebuf`).
#[allow(dead_code)]
static LINEBUF: crate::globals::GlobalCell<Option<Vec<crate::types_defs::ScharT>>> =
    crate::globals::GlobalCell::new(None);
/// Scratch composed attribute line (`attrbuf`).
#[allow(dead_code)]
static ATTRBUF: crate::globals::GlobalCell<Option<Vec<crate::types_defs::SattrT>>> =
    crate::globals::GlobalCell::new(None);
/// Allocated scratch-line width (`bufsize`).
#[allow(dead_code)]
static BUFSIZE: crate::globals::GlobalCell<usize> =
    crate::globals::GlobalCell::new(0);
/// Message separator screen row (`msg_sep_row`).
#[allow(dead_code)]
static MSG_SEP_ROW: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(-1);

/// Whether the compositor should draw (`ui_comp_should_draw`).
#[must_use]
pub fn ui_comp_should_draw() -> bool {
    unsafe { *COMPOSED_UIS.get_mut() != 0 && *VALID_SCREEN.get_mut() }
}

/// Attach a UI to the compositor (`ui_comp_attach`).
pub fn ui_comp_attach(ui: &mut crate::ui::RemoteUI) {
    unsafe {
        let count = COMPOSED_UIS.get_mut();
        *count = count.wrapping_add(1);
    }
    ui.composed = true;
}

/// Detach a UI from the compositor (`ui_comp_detach`).
pub fn ui_comp_detach(ui: &mut crate::ui::RemoteUI) {
    let count = unsafe { COMPOSED_UIS.get_mut() };
    *count = count.wrapping_sub(1);
    if *count == 0 {
        *unsafe { LINEBUF.get_mut() } = None;
        *unsafe { ATTRBUF.get_mut() } = None;
        *unsafe { BUFSIZE.get_mut() } = 0;
    }
    ui.composed = false;
}

/// Mark the composed screen valid or invalid (`ui_comp_set_screen_valid`).
///
/// Returns the previous validity; invalidating also hides the message
/// separator until the screen is cleared.
pub fn ui_comp_set_screen_valid(valid: bool) -> bool {
    let old = std::mem::replace(unsafe { VALID_SCREEN.get_mut() }, valid);
    if !valid {
        *unsafe { MSG_SEP_ROW.get_mut() } = -1;
    }
    old
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CompositorStateGuard {
        composed: i32,
        valid: bool,
    }

    struct CompositorBuffersGuard {
        line: Option<Vec<crate::types_defs::ScharT>>,
        attrs: Option<Vec<crate::types_defs::SattrT>>,
        size: usize,
    }

    struct MsgSepGuard(i32);

    impl MsgSepGuard {
        fn install(value: i32) -> Self {
            Self(std::mem::replace(
                unsafe { MSG_SEP_ROW.get_mut() },
                value,
            ))
        }
    }

    impl Drop for MsgSepGuard {
        fn drop(&mut self) {
            *unsafe { MSG_SEP_ROW.get_mut() } = self.0;
        }
    }

    impl CompositorBuffersGuard {
        fn install() -> Self {
            Self {
                line: unsafe { LINEBUF.get_mut() }.replace(vec![1, 2]),
                attrs: unsafe { ATTRBUF.get_mut() }.replace(vec![3, 4]),
                size: std::mem::replace(unsafe { BUFSIZE.get_mut() }, 2),
            }
        }
    }

    impl Drop for CompositorBuffersGuard {
        fn drop(&mut self) {
            *unsafe { LINEBUF.get_mut() } = self.line.take();
            *unsafe { ATTRBUF.get_mut() } = self.attrs.take();
            *unsafe { BUFSIZE.get_mut() } = self.size;
        }
    }

    impl CompositorStateGuard {
        fn install(composed: i32, valid: bool) -> Self {
            let previous = Self {
                composed: unsafe { *COMPOSED_UIS.get_mut() },
                valid: unsafe { *VALID_SCREEN.get_mut() },
            };
            unsafe {
                *COMPOSED_UIS.get_mut() = composed;
                *VALID_SCREEN.get_mut() = valid;
            }
            previous
        }
    }

    impl Drop for CompositorStateGuard {
        fn drop(&mut self) {
            unsafe {
                *COMPOSED_UIS.get_mut() = self.composed;
                *VALID_SCREEN.get_mut() = self.valid;
            }
        }
    }

    #[test]
    fn ui_comp_should_draw_requires_a_composed_ui_and_valid_screen() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = CompositorStateGuard::install(0, true);
        assert!(!ui_comp_should_draw());

        unsafe { *COMPOSED_UIS.get_mut() = 1 };
        assert!(ui_comp_should_draw());

        unsafe { *VALID_SCREEN.get_mut() = false };
        assert!(!ui_comp_should_draw());
    }

    #[test]
    fn ui_comp_attach_marks_the_ui_and_increments_the_count() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = CompositorStateGuard::install(2, true);
        let mut ui = crate::ui::RemoteUI::default();

        ui_comp_attach(&mut ui);

        assert!(ui.composed);
        assert_eq!(unsafe { *COMPOSED_UIS.get_mut() }, 3);
        assert!(ui_comp_should_draw());
    }

    #[test]
    fn ui_comp_detach_releases_scratch_buffers_after_the_last_ui() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = CompositorStateGuard::install(1, true);
        let _buffers = CompositorBuffersGuard::install();
        let mut ui = crate::ui::RemoteUI {
            composed: true,
            ..Default::default()
        };

        ui_comp_detach(&mut ui);

        assert!(!ui.composed);
        assert_eq!(unsafe { *COMPOSED_UIS.get_mut() }, 0);
        assert!(unsafe { LINEBUF.get_mut() }.is_none());
        assert!(unsafe { ATTRBUF.get_mut() }.is_none());
        assert_eq!(unsafe { *BUFSIZE.get_mut() }, 0);
    }

    #[test]
    fn ui_comp_set_screen_valid_returns_old_state_and_resets_separator() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = CompositorStateGuard::install(1, true);
        let _separator = MsgSepGuard::install(12);

        assert!(ui_comp_set_screen_valid(false));
        assert!(!ui_comp_should_draw());
        assert_eq!(unsafe { *MSG_SEP_ROW.get_mut() }, -1);

        assert!(!ui_comp_set_screen_valid(true));
        assert!(ui_comp_should_draw());
        assert_eq!(unsafe { *MSG_SEP_ROW.get_mut() }, -1);
    }
}
