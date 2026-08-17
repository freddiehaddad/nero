//! Translated from `src/nvim/vterm/pen.c`.

/// RGB triple without `VTermColor` metadata (`VTermRGB`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VTermRgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

/// Current terminal pen (`VTermPen`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VTermPen {
    pub fg: crate::vterm_defs::VTermColor,
    pub bg: crate::vterm_defs::VTermColor,
    pub uri: i32,
    pub bold: bool,
    pub underline: u8,
    pub italic: bool,
    pub blink: bool,
    pub reverse: bool,
    pub conceal: bool,
    pub strike: bool,
    pub font: u8,
    pub small: bool,
    pub baseline: u8,
    pub dim: bool,
    pub overline: bool,
}

#[allow(dead_code)]
const ANSI_COLORS: [VTermRgb; 16] = [
    VTermRgb { red: 0, green: 0, blue: 0 },
    VTermRgb { red: 224, green: 0, blue: 0 },
    VTermRgb { red: 0, green: 224, blue: 0 },
    VTermRgb { red: 224, green: 224, blue: 0 },
    VTermRgb { red: 0, green: 0, blue: 224 },
    VTermRgb { red: 224, green: 0, blue: 224 },
    VTermRgb { red: 0, green: 224, blue: 224 },
    VTermRgb { red: 224, green: 224, blue: 224 },
    VTermRgb { red: 128, green: 128, blue: 128 },
    VTermRgb { red: 255, green: 64, blue: 64 },
    VTermRgb { red: 64, green: 255, blue: 64 },
    VTermRgb { red: 255, green: 255, blue: 64 },
    VTermRgb { red: 64, green: 64, blue: 255 },
    VTermRgb { red: 255, green: 64, blue: 255 },
    VTermRgb { red: 64, green: 255, blue: 255 },
    VTermRgb { red: 255, green: 255, blue: 255 },
];

#[allow(dead_code)]
const RAMP6: [u8; 6] = [0x00, 0x33, 0x66, 0x99, 0xCC, 0xFF];

#[allow(dead_code)]
const RAMP24: [u8; 24] = [
    0x00, 0x0B, 0x16, 0x21, 0x2C, 0x37, 0x42, 0x4D, 0x58, 0x63, 0x6E, 0x79,
    0x85, 0x90, 0x9B, 0xA6, 0xB1, 0xBC, 0xC7, 0xD2, 0xDD, 0xE8, 0xF3, 0xFF,
];

/// Looks up one of the fixed ANSI colors
/// (`lookup_default_colour_ansi`).
pub fn lookup_default_colour_ansi(
    index: i64,
    color: &mut crate::vterm_defs::VTermColor,
) {
    let Ok(index) = usize::try_from(index) else {
        return;
    };
    let Some(rgb) = ANSI_COLORS.get(index) else {
        return;
    };
    crate::vterm_defs::vterm_color_rgb(color, rgb.red, rgb.green, rgb.blue);
}

/// The sixteen mutable ANSI palette entries from `VTermState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VTermPalette {
    pub colors: [crate::vterm_defs::VTermColor; 16],
}

/// Pen and color fields owned by `VTermState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VTermPenState {
    pub pen: VTermPen,
    pub saved_pen: VTermPen,
    pub default_fg: crate::vterm_defs::VTermColor,
    pub default_bg: crate::vterm_defs::VTermColor,
    pub palette: VTermPalette,
    pub bold_is_highbright: bool,
}

/// Pen attribute callback from `VTermStateCallbacks::setpenattr`.
pub trait VTermPenCallbacks {
    fn set_pen_attr(
        &mut self,
        _attr: crate::vterm_defs::VTermAttr,
        _value: &crate::vterm_defs::VTermValue<'_>,
    ) -> bool {
        false
    }
}

impl VTermPenCallbacks for () {}

pub fn setpenattr<C: VTermPenCallbacks>(
    callbacks: &mut C,
    attr: crate::vterm_defs::VTermAttr,
    value: &crate::vterm_defs::VTermValue<'_>,
) {
    debug_assert_eq!(
        value.value_type(),
        crate::vterm::core::vterm_get_attr_type(attr)
    );
    let _ = callbacks.set_pen_attr(attr, value);
}

#[allow(dead_code)]
fn setpenattr_bool<C: VTermPenCallbacks>(
    callbacks: &mut C,
    attr: crate::vterm_defs::VTermAttr,
    boolean: bool,
) {
    let value = crate::vterm_defs::VTermValue::Boolean(i32::from(boolean));
    let _ = callbacks.set_pen_attr(attr, &value);
}

#[allow(dead_code)]
fn setpenattr_int<C: VTermPenCallbacks>(
    callbacks: &mut C,
    attr: crate::vterm_defs::VTermAttr,
    number: i32,
) {
    let value = crate::vterm_defs::VTermValue::Number(number);
    let _ = callbacks.set_pen_attr(attr, &value);
}

#[allow(dead_code)]
fn setpenattr_col<C: VTermPenCallbacks>(
    callbacks: &mut C,
    attr: crate::vterm_defs::VTermAttr,
    color: crate::vterm_defs::VTermColor,
) {
    let value = crate::vterm_defs::VTermValue::Color(color);
    let _ = callbacks.set_pen_attr(attr, &value);
}

#[allow(dead_code)]
fn set_pen_col_ansi<C: VTermPenCallbacks>(
    state: &mut VTermPenState,
    callbacks: &mut C,
    attr: crate::vterm_defs::VTermAttr,
    color: i64,
) {
    let target = if attr == crate::vterm_defs::VTermAttr::Background {
        &mut state.pen.bg
    } else {
        &mut state.pen.fg
    };
    crate::vterm_defs::vterm_color_indexed(target, color as u8);
    setpenattr_col(callbacks, attr, *target);
}

#[allow(dead_code)]
fn apply_sgr_intensity<C: VTermPenCallbacks>(
    state: &mut VTermPenState,
    callbacks: &mut C,
    argument: u32,
) -> bool {
    match argument {
        1 => {
            let foreground = state.pen.fg;
            state.pen.bold = true;
            setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Bold, true);
            if !foreground.is_default_fg()
                && foreground.is_indexed()
                && foreground.index < 8
                && state.bold_is_highbright
            {
                set_pen_col_ansi(
                    state,
                    callbacks,
                    crate::vterm_defs::VTermAttr::Foreground,
                    i64::from(foreground.index + 8),
                );
            }
            true
        }
        2 => {
            state.pen.dim = true;
            setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Dim, true);
            true
        }
        22 => {
            state.pen.bold = false;
            setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Bold, false);
            state.pen.dim = false;
            setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Dim, false);
            true
        }
        _ => false,
    }
}

#[allow(dead_code)]
fn apply_sgr_italic<C: VTermPenCallbacks>(
    state: &mut VTermPenState,
    callbacks: &mut C,
    argument: u32,
) -> bool {
    let italic = match argument {
        3 => true,
        23 => false,
        _ => return false,
    };
    state.pen.italic = italic;
    setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Italic, italic);
    true
}

#[allow(dead_code)]
fn apply_sgr_underline<C: VTermPenCallbacks>(
    state: &mut VTermPenState,
    callbacks: &mut C,
    args: &[crate::vterm::parser::CsiArg],
    index: usize,
) -> Option<usize> {
    let argument = crate::vterm::parser::csi_arg(*args.get(index)?);
    let mut consumed = 1;
    state.pen.underline = match argument {
        4 => {
            let mut underline = crate::vterm_defs::VTERM_UNDERLINE_SINGLE;
            if crate::vterm::parser::csi_arg_has_more(args[index])
                && let Some(&subparameter) = args.get(index + 1)
            {
                consumed = 2;
                underline = match crate::vterm::parser::csi_arg(subparameter) {
                    0 => crate::vterm_defs::VTERM_UNDERLINE_OFF,
                    1 => crate::vterm_defs::VTERM_UNDERLINE_SINGLE,
                    2 => crate::vterm_defs::VTERM_UNDERLINE_DOUBLE,
                    3 => crate::vterm_defs::VTERM_UNDERLINE_CURLY,
                    _ => underline,
                };
            }
            underline
        }
        21 => crate::vterm_defs::VTERM_UNDERLINE_DOUBLE,
        24 => crate::vterm_defs::VTERM_UNDERLINE_OFF,
        _ => return None,
    };
    setpenattr_int(
        callbacks,
        crate::vterm_defs::VTermAttr::Underline,
        i32::from(state.pen.underline),
    );
    Some(consumed)
}

#[allow(dead_code)]
fn apply_sgr_visibility<C: VTermPenCallbacks>(
    state: &mut VTermPenState,
    callbacks: &mut C,
    argument: u32,
) -> bool {
    let (attr, enabled) = match argument {
        5 => (crate::vterm_defs::VTermAttr::Blink, true),
        25 => (crate::vterm_defs::VTermAttr::Blink, false),
        7 => (crate::vterm_defs::VTermAttr::Reverse, true),
        27 => (crate::vterm_defs::VTermAttr::Reverse, false),
        8 => (crate::vterm_defs::VTermAttr::Conceal, true),
        28 => (crate::vterm_defs::VTermAttr::Conceal, false),
        9 => (crate::vterm_defs::VTermAttr::Strike, true),
        29 => (crate::vterm_defs::VTermAttr::Strike, false),
        _ => return false,
    };
    match attr {
        crate::vterm_defs::VTermAttr::Blink => state.pen.blink = enabled,
        crate::vterm_defs::VTermAttr::Reverse => state.pen.reverse = enabled,
        crate::vterm_defs::VTermAttr::Conceal => state.pen.conceal = enabled,
        crate::vterm_defs::VTermAttr::Strike => state.pen.strike = enabled,
        _ => unreachable!(),
    }
    setpenattr_bool(callbacks, attr, enabled);
    true
}

#[allow(dead_code)]
fn apply_sgr_font<C: VTermPenCallbacks>(
    state: &mut VTermPenState,
    callbacks: &mut C,
    argument: u32,
) -> bool {
    if !(10..=19).contains(&argument) {
        return false;
    }
    state.pen.font = (argument - 10) as u8;
    setpenattr_int(
        callbacks,
        crate::vterm_defs::VTermAttr::Font,
        i32::from(state.pen.font),
    );
    true
}

#[allow(dead_code)]
fn apply_sgr_overline<C: VTermPenCallbacks>(
    state: &mut VTermPenState,
    callbacks: &mut C,
    argument: u32,
) -> bool {
    let overline = match argument {
        53 => true,
        55 => false,
        _ => return false,
    };
    state.pen.overline = overline;
    setpenattr_bool(
        callbacks,
        crate::vterm_defs::VTermAttr::Overline,
        overline,
    );
    true
}

#[allow(dead_code)]
fn apply_sgr_baseline<C: VTermPenCallbacks>(
    state: &mut VTermPenState,
    callbacks: &mut C,
    argument: u32,
) -> bool {
    let baseline = match argument {
        73 => crate::vterm_defs::VTERM_BASELINE_RAISE,
        74 => crate::vterm_defs::VTERM_BASELINE_LOWER,
        75 => crate::vterm_defs::VTERM_BASELINE_NORMAL,
        _ => return false,
    };
    state.pen.small = argument != 75;
    state.pen.baseline = baseline;
    setpenattr_bool(
        callbacks,
        crate::vterm_defs::VTermAttr::Small,
        state.pen.small,
    );
    setpenattr_int(
        callbacks,
        crate::vterm_defs::VTermAttr::Baseline,
        i32::from(baseline),
    );
    true
}

#[allow(dead_code)]
fn apply_sgr_standard_color<C: VTermPenCallbacks>(
    state: &mut VTermPenState,
    callbacks: &mut C,
    argument: u32,
) -> bool {
    match argument {
        30..=37 => {
            let mut value = argument - 30;
            if state.pen.bold && state.bold_is_highbright {
                value += 8;
            }
            set_pen_col_ansi(
                state,
                callbacks,
                crate::vterm_defs::VTermAttr::Foreground,
                i64::from(value),
            );
            true
        }
        40..=47 => {
            set_pen_col_ansi(
                state,
                callbacks,
                crate::vterm_defs::VTermAttr::Background,
                i64::from(argument - 40),
            );
            true
        }
        _ => false,
    }
}

#[allow(dead_code)]
fn apply_sgr_high_color<C: VTermPenCallbacks>(
    state: &mut VTermPenState,
    callbacks: &mut C,
    argument: u32,
) -> bool {
    match argument {
        90..=97 => {
            set_pen_col_ansi(
                state,
                callbacks,
                crate::vterm_defs::VTermAttr::Foreground,
                i64::from(argument - 90 + 8),
            );
            true
        }
        100..=107 => {
            set_pen_col_ansi(
                state,
                callbacks,
                crate::vterm_defs::VTermAttr::Background,
                i64::from(argument - 100 + 8),
            );
            true
        }
        _ => false,
    }
}

#[allow(dead_code)]
fn apply_sgr_default_color<C: VTermPenCallbacks>(
    state: &mut VTermPenState,
    callbacks: &mut C,
    argument: u32,
) -> bool {
    match argument {
        39 => {
            state.pen.fg = state.default_fg;
            setpenattr_col(
                callbacks,
                crate::vterm_defs::VTermAttr::Foreground,
                state.pen.fg,
            );
            true
        }
        49 => {
            state.pen.bg = state.default_bg;
            setpenattr_col(
                callbacks,
                crate::vterm_defs::VTermAttr::Background,
                state.pen.bg,
            );
            true
        }
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SgrColorResult {
    Unhandled,
    Applied(usize),
    Truncated,
}

#[allow(dead_code)]
fn apply_sgr_alternate_color<C: VTermPenCallbacks>(
    state: &mut VTermPenState,
    callbacks: &mut C,
    args: &[crate::vterm::parser::CsiArg],
    index: usize,
) -> SgrColorResult {
    let Some(&argument) = args.get(index) else {
        return SgrColorResult::Unhandled;
    };
    let attr = match crate::vterm::parser::csi_arg(argument) {
        38 => crate::vterm_defs::VTermAttr::Foreground,
        48 => crate::vterm_defs::VTermAttr::Background,
        _ => return SgrColorResult::Unhandled,
    };
    let Some(&palette) = args.get(index + 1) else {
        return SgrColorResult::Truncated;
    };
    let target = if attr == crate::vterm_defs::VTermAttr::Foreground {
        &mut state.pen.fg
    } else {
        &mut state.pen.bg
    };
    let payload = args.get(index + 2..).unwrap_or_default();
    let payload_consumed = lookup_colour(
        crate::vterm::parser::csi_arg(palette) as i32,
        payload,
        target,
    );
    setpenattr_col(callbacks, attr, *target);
    SgrColorResult::Applied(2 + payload_consumed)
}

/// Applies an SGR argument list (`vterm_state_setpen`).
pub fn vterm_state_setpen<C: VTermPenCallbacks>(
    state: &mut VTermPenState,
    args: &[crate::vterm::parser::CsiArg],
    callbacks: &mut C,
) {
    let mut index = 0;
    while index < args.len() {
        let argument = crate::vterm::parser::csi_arg(args[index]);
        let mut consumed = 1;

        if argument == crate::vterm::parser::CSI_ARG_MISSING || argument == 0 {
            vterm_state_resetpen(state, callbacks);
        } else if apply_sgr_intensity(state, callbacks, argument)
            || apply_sgr_italic(state, callbacks, argument)
            || apply_sgr_visibility(state, callbacks, argument)
            || apply_sgr_font(state, callbacks, argument)
            || apply_sgr_overline(state, callbacks, argument)
            || apply_sgr_baseline(state, callbacks, argument)
            || apply_sgr_standard_color(state, callbacks, argument)
            || apply_sgr_high_color(state, callbacks, argument)
            || apply_sgr_default_color(state, callbacks, argument)
        {
        } else if let Some(underline_consumed) =
            apply_sgr_underline(state, callbacks, args, index)
        {
            consumed = underline_consumed;
        } else {
            match apply_sgr_alternate_color(state, callbacks, args, index) {
                SgrColorResult::Unhandled => {}
                SgrColorResult::Applied(color_consumed) => consumed = color_consumed,
                SgrColorResult::Truncated => return,
            }
        }

        index += consumed - 1;
        loop {
            let has_more = crate::vterm::parser::csi_arg_has_more(args[index]);
            index += 1;
            if !has_more || index >= args.len() {
                break;
            }
        }
    }
}

#[allow(dead_code)]
fn append_pen_color(
    color: &crate::vterm_defs::VTermColor,
    foreground: bool,
    args: &mut Vec<crate::vterm::parser::CsiArg>,
) {
    if (foreground && color.is_default_fg()) || (!foreground && color.is_default_bg()) {
        return;
    }

    if color.is_indexed() {
        let index = color.index;
        if index < 8 {
            args.push(u32::from(index) + if foreground { 30 } else { 40 });
        } else if index < 16 {
            args.push(u32::from(index - 8) + if foreground { 90 } else { 100 });
        } else {
            args.push(
                crate::vterm::parser::CSI_ARG_FLAG_MORE
                    | if foreground { 38 } else { 48 },
            );
            args.push(crate::vterm::parser::CSI_ARG_FLAG_MORE | 5);
            args.push(u32::from(index));
        }
    } else if color.is_rgb() {
        args.push(
            crate::vterm::parser::CSI_ARG_FLAG_MORE
                | if foreground { 38 } else { 48 },
        );
        args.push(crate::vterm::parser::CSI_ARG_FLAG_MORE | 2);
        args.push(crate::vterm::parser::CSI_ARG_FLAG_MORE | u32::from(color.red));
        args.push(crate::vterm::parser::CSI_ARG_FLAG_MORE | u32::from(color.green));
        args.push(u32::from(color.blue));
    }
}

/// Serializes the active pen as SGR arguments (`vterm_state_getpen`).
#[must_use]
pub fn vterm_state_getpen(state: &VTermPenState) -> Vec<crate::vterm::parser::CsiArg> {
    let mut args = Vec::new();
    if state.pen.bold {
        args.push(1);
    }
    if state.pen.dim {
        args.push(2);
    }
    if state.pen.italic {
        args.push(3);
    }
    if state.pen.underline == crate::vterm_defs::VTERM_UNDERLINE_SINGLE {
        args.push(4);
    }
    if state.pen.underline == crate::vterm_defs::VTERM_UNDERLINE_CURLY {
        args.push(crate::vterm::parser::CSI_ARG_FLAG_MORE | 4);
        args.push(3);
    }
    if state.pen.blink {
        args.push(5);
    }
    if state.pen.reverse {
        args.push(7);
    }
    if state.pen.conceal {
        args.push(8);
    }
    if state.pen.strike {
        args.push(9);
    }
    if state.pen.font != 0 {
        args.push(10 + u32::from(state.pen.font));
    }
    if state.pen.underline == crate::vterm_defs::VTERM_UNDERLINE_DOUBLE {
        args.push(21);
    }
    append_pen_color(&state.pen.fg, true, &mut args);
    append_pen_color(&state.pen.bg, false, &mut args);
    if state.pen.overline {
        args.push(53);
    }
    if state.pen.small {
        if state.pen.baseline == crate::vterm_defs::VTERM_BASELINE_RAISE {
            args.push(73);
        } else if state.pen.baseline == crate::vterm_defs::VTERM_BASELINE_LOWER {
            args.push(74);
        }
    }
    args
}

/// Sets one pen attribute after validating its value type
/// (`vterm_state_set_penattr`).
pub fn vterm_state_set_penattr<C: VTermPenCallbacks>(
    state: &mut VTermPenState,
    attr: crate::vterm_defs::VTermAttr,
    value_type: crate::vterm_defs::VTermValueType,
    value: Option<&crate::vterm_defs::VTermValue<'_>>,
    callbacks: &mut C,
) -> i32 {
    let Some(value) = value else {
        return 0;
    };
    if value_type != crate::vterm::core::vterm_get_attr_type(attr)
        || value.value_type() != value_type
    {
        return 0;
    }

    match (attr, value) {
        (crate::vterm_defs::VTermAttr::Bold, crate::vterm_defs::VTermValue::Boolean(value)) => {
            state.pen.bold = *value != 0;
        }
        (
            crate::vterm_defs::VTermAttr::Underline,
            crate::vterm_defs::VTermValue::Number(value),
        ) => state.pen.underline = *value as u8 & 0x03,
        (crate::vterm_defs::VTermAttr::Italic, crate::vterm_defs::VTermValue::Boolean(value)) => {
            state.pen.italic = *value != 0;
        }
        (crate::vterm_defs::VTermAttr::Blink, crate::vterm_defs::VTermValue::Boolean(value)) => {
            state.pen.blink = *value != 0;
        }
        (
            crate::vterm_defs::VTermAttr::Reverse,
            crate::vterm_defs::VTermValue::Boolean(value),
        ) => state.pen.reverse = *value != 0,
        (
            crate::vterm_defs::VTermAttr::Conceal,
            crate::vterm_defs::VTermValue::Boolean(value),
        ) => state.pen.conceal = *value != 0,
        (crate::vterm_defs::VTermAttr::Strike, crate::vterm_defs::VTermValue::Boolean(value)) => {
            state.pen.strike = *value != 0;
        }
        (crate::vterm_defs::VTermAttr::Font, crate::vterm_defs::VTermValue::Number(value)) => {
            state.pen.font = *value as u8 & 0x0F;
        }
        (
            crate::vterm_defs::VTermAttr::Foreground,
            crate::vterm_defs::VTermValue::Color(value),
        ) => state.pen.fg = *value,
        (
            crate::vterm_defs::VTermAttr::Background,
            crate::vterm_defs::VTermValue::Color(value),
        ) => state.pen.bg = *value,
        (crate::vterm_defs::VTermAttr::Small, crate::vterm_defs::VTermValue::Boolean(value)) => {
            state.pen.small = *value != 0;
        }
        (
            crate::vterm_defs::VTermAttr::Baseline,
            crate::vterm_defs::VTermValue::Number(value),
        ) => state.pen.baseline = *value as u8 & 0x03,
        (crate::vterm_defs::VTermAttr::Uri, crate::vterm_defs::VTermValue::Number(value)) => {
            state.pen.uri = *value;
        }
        (crate::vterm_defs::VTermAttr::Dim, crate::vterm_defs::VTermValue::Boolean(value)) => {
            state.pen.dim = *value != 0;
        }
        (
            crate::vterm_defs::VTermAttr::Overline,
            crate::vterm_defs::VTermValue::Boolean(value),
        ) => state.pen.overline = *value != 0,
        _ => return 0,
    }
    let _ = callbacks.set_pen_attr(attr, value);
    1
}

impl Default for VTermPenState {
    fn default() -> Self {
        Self {
            pen: VTermPen::default(),
            saved_pen: VTermPen::default(),
            default_fg: crate::vterm_defs::VTermColor::default(),
            default_bg: crate::vterm_defs::VTermColor::default(),
            palette: VTermPalette {
                colors: [crate::vterm_defs::VTermColor::default(); 16],
            },
            bold_is_highbright: false,
        }
    }
}

/// Updates the terminal's default colors
/// (`vterm_state_set_default_colors`).
pub fn vterm_state_set_default_colors(
    state: &mut VTermPenState,
    default_fg: Option<&crate::vterm_defs::VTermColor>,
    default_bg: Option<&crate::vterm_defs::VTermColor>,
) {
    if let Some(default_fg) = default_fg {
        state.default_fg = *default_fg;
        state.default_fg.color_type =
            (state.default_fg.color_type & !crate::vterm_defs::VTERM_COLOR_DEFAULT_MASK)
                | crate::vterm_defs::VTERM_COLOR_DEFAULT_FG;
    }
    if let Some(default_bg) = default_bg {
        state.default_bg = *default_bg;
        state.default_bg.color_type =
            (state.default_bg.color_type & !crate::vterm_defs::VTERM_COLOR_DEFAULT_MASK)
                | crate::vterm_defs::VTERM_COLOR_DEFAULT_BG;
    }
}

/// Initializes default foreground/background and ANSI colors
/// (`vterm_state_newpen`).
pub fn vterm_state_newpen(state: &mut VTermPenState) {
    let mut default_fg = crate::vterm_defs::VTermColor::default();
    crate::vterm_defs::vterm_color_rgb(&mut default_fg, 240, 240, 240);
    let mut default_bg = crate::vterm_defs::VTermColor::default();
    crate::vterm_defs::vterm_color_rgb(&mut default_bg, 0, 0, 0);
    vterm_state_set_default_colors(state, Some(&default_fg), Some(&default_bg));
    for (index, color) in state.palette.colors.iter_mut().enumerate() {
        lookup_default_colour_ansi(index as i64, color);
    }
}

/// Resets every pen attribute and restores default colors
/// (`vterm_state_resetpen`).
pub fn vterm_state_resetpen<C: VTermPenCallbacks>(
    state: &mut VTermPenState,
    callbacks: &mut C,
) {
    state.pen.bold = false;
    setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Bold, false);
    state.pen.underline = 0;
    setpenattr_int(callbacks, crate::vterm_defs::VTermAttr::Underline, 0);
    state.pen.italic = false;
    setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Italic, false);
    state.pen.blink = false;
    setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Blink, false);
    state.pen.reverse = false;
    setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Reverse, false);
    state.pen.conceal = false;
    setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Conceal, false);
    state.pen.strike = false;
    setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Strike, false);
    state.pen.font = 0;
    setpenattr_int(callbacks, crate::vterm_defs::VTermAttr::Font, 0);
    state.pen.small = false;
    setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Small, false);
    state.pen.baseline = 0;
    setpenattr_int(callbacks, crate::vterm_defs::VTermAttr::Baseline, 0);
    state.pen.dim = false;
    setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Dim, false);
    state.pen.overline = false;
    setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Overline, false);
    state.pen.fg = state.default_fg;
    setpenattr_col(
        callbacks,
        crate::vterm_defs::VTermAttr::Foreground,
        state.default_fg,
    );
    state.pen.bg = state.default_bg;
    setpenattr_col(
        callbacks,
        crate::vterm_defs::VTermAttr::Background,
        state.default_bg,
    );
    state.pen.uri = 0;
    setpenattr_int(callbacks, crate::vterm_defs::VTermAttr::Uri, 0);
}

/// Saves or restores the pen (`vterm_state_savepen`).
pub fn vterm_state_savepen<C: VTermPenCallbacks>(
    state: &mut VTermPenState,
    save: i32,
    callbacks: &mut C,
) {
    if save != 0 {
        state.saved_pen = state.pen;
        return;
    }

    state.pen = state.saved_pen;
    setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Bold, state.pen.bold);
    setpenattr_int(
        callbacks,
        crate::vterm_defs::VTermAttr::Underline,
        i32::from(state.pen.underline),
    );
    setpenattr_bool(
        callbacks,
        crate::vterm_defs::VTermAttr::Italic,
        state.pen.italic,
    );
    setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Blink, state.pen.blink);
    setpenattr_bool(
        callbacks,
        crate::vterm_defs::VTermAttr::Reverse,
        state.pen.reverse,
    );
    setpenattr_bool(
        callbacks,
        crate::vterm_defs::VTermAttr::Conceal,
        state.pen.conceal,
    );
    setpenattr_bool(
        callbacks,
        crate::vterm_defs::VTermAttr::Strike,
        state.pen.strike,
    );
    setpenattr_int(
        callbacks,
        crate::vterm_defs::VTermAttr::Font,
        i32::from(state.pen.font),
    );
    setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Small, state.pen.small);
    setpenattr_int(
        callbacks,
        crate::vterm_defs::VTermAttr::Baseline,
        i32::from(state.pen.baseline),
    );
    setpenattr_bool(callbacks, crate::vterm_defs::VTermAttr::Dim, state.pen.dim);
    setpenattr_bool(
        callbacks,
        crate::vterm_defs::VTermAttr::Overline,
        state.pen.overline,
    );
    setpenattr_col(
        callbacks,
        crate::vterm_defs::VTermAttr::Foreground,
        state.pen.fg,
    );
    setpenattr_col(
        callbacks,
        crate::vterm_defs::VTermAttr::Background,
        state.pen.bg,
    );
    setpenattr_int(callbacks, crate::vterm_defs::VTermAttr::Uri, state.pen.uri);
}

/// Replaces one mutable ANSI palette entry
/// (`vterm_state_set_palette_color`).
pub fn vterm_state_set_palette_color(
    state: &mut VTermPenState,
    index: i32,
    color: &crate::vterm_defs::VTermColor,
) {
    let Ok(index) = usize::try_from(index) else {
        return;
    };
    if let Some(slot) = state.palette.colors.get_mut(index) {
        *slot = *color;
    }
}

/// Converts a color to RGB and clears default-color metadata
/// (`vterm_state_convert_color_to_rgb`).
pub fn vterm_state_convert_color_to_rgb(
    state: &VTermPenState,
    color: &mut crate::vterm_defs::VTermColor,
) {
    if color.is_indexed() {
        let index = color.index;
        let _ = lookup_colour_palette(&state.palette, i64::from(index), color);
    }
    color.color_type &= crate::vterm_defs::VTERM_COLOR_TYPE_MASK;
}

impl Default for VTermPalette {
    fn default() -> Self {
        let mut colors = [crate::vterm_defs::VTermColor::default(); 16];
        for (index, color) in colors.iter_mut().enumerate() {
            lookup_default_colour_ansi(index as i64, color);
        }
        Self { colors }
    }
}

/// Looks up a mutable ANSI palette entry (`lookup_colour_ansi`).
#[must_use]
pub fn lookup_colour_ansi(
    palette: &VTermPalette,
    index: i64,
    color: &mut crate::vterm_defs::VTermColor,
) -> bool {
    let Ok(index) = usize::try_from(index) else {
        return false;
    };
    let Some(found) = palette.colors.get(index) else {
        return false;
    };
    *color = *found;
    true
}

/// Resolves an xterm 256-color palette index
/// (`lookup_colour_palette`).
#[must_use]
pub fn lookup_colour_palette(
    palette: &VTermPalette,
    mut index: i64,
    color: &mut crate::vterm_defs::VTermColor,
) -> bool {
    if (0..16).contains(&index) {
        return lookup_colour_ansi(palette, index, color);
    }
    if (16..232).contains(&index) {
        index -= 16;
        let index = index as usize;
        crate::vterm_defs::vterm_color_rgb(
            color,
            RAMP6[index / 6 / 6 % 6],
            RAMP6[index / 6 % 6],
            RAMP6[index % 6],
        );
        return true;
    }
    if (232..256).contains(&index) {
        index -= 232;
        let value = RAMP24[index as usize];
        crate::vterm_defs::vterm_color_rgb(color, value, value, value);
        return true;
    }
    false
}

/// Parses an SGR color payload (`lookup_colour`).
#[must_use]
pub fn lookup_colour(
    palette: i32,
    args: &[crate::vterm::parser::CsiArg],
    color: &mut crate::vterm_defs::VTermColor,
) -> usize {
    match palette {
        2 => {
            if args.len() < 3 {
                return args.len();
            }
            crate::vterm_defs::vterm_color_rgb(
                color,
                crate::vterm::parser::csi_arg(args[0]) as u8,
                crate::vterm::parser::csi_arg(args[1]) as u8,
                crate::vterm::parser::csi_arg(args[2]) as u8,
            );
            3
        }
        5 => {
            let Some(&index) = args.first() else {
                return 0;
            };
            if crate::vterm::parser::csi_arg_is_missing(index) {
                return 1;
            }
            crate::vterm_defs::vterm_color_indexed(color, index as u8);
            1
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type CapturedPenAttr = (
        crate::vterm_defs::VTermAttr,
        crate::vterm_defs::VTermValue<'static>,
    );

    #[derive(Default)]
    struct PenCapture(Vec<CapturedPenAttr>);

    impl VTermPenCallbacks for PenCapture {
        fn set_pen_attr(
            &mut self,
            attr: crate::vterm_defs::VTermAttr,
            value: &crate::vterm_defs::VTermValue<'_>,
        ) -> bool {
            let owned = match value {
                crate::vterm_defs::VTermValue::Boolean(value) => {
                    crate::vterm_defs::VTermValue::Boolean(*value)
                }
                crate::vterm_defs::VTermValue::Number(value) => {
                    crate::vterm_defs::VTermValue::Number(*value)
                }
                crate::vterm_defs::VTermValue::Color(value) => {
                    crate::vterm_defs::VTermValue::Color(*value)
                }
                crate::vterm_defs::VTermValue::String(_) => unreachable!(),
            };
            self.0.push((attr, owned));
            true
        }
    }

    #[test]
    fn default_pen_callback_declines_attributes() {
        assert!(!().set_pen_attr(
            crate::vterm_defs::VTermAttr::Bold,
            &crate::vterm_defs::VTermValue::Boolean(1),
        ));
    }

    #[test]
    fn generic_setpenattr_forwards_typed_value() {
        let mut capture = PenCapture::default();
        setpenattr(
            &mut capture,
            crate::vterm_defs::VTermAttr::Bold,
            &crate::vterm_defs::VTermValue::Boolean(1),
        );
        assert_eq!(capture.0.len(), 1);
    }

    #[test]
    fn pen_attribute_helpers_construct_the_matching_value_variants() {
        let mut capture = PenCapture::default();
        let color = crate::vterm_defs::VTermColor {
            red: 1,
            green: 2,
            blue: 3,
            ..Default::default()
        };
        setpenattr_bool(
            &mut capture,
            crate::vterm_defs::VTermAttr::Bold,
            true,
        );
        setpenattr_int(
            &mut capture,
            crate::vterm_defs::VTermAttr::Font,
            4,
        );
        setpenattr_col(
            &mut capture,
            crate::vterm_defs::VTermAttr::Foreground,
            color,
        );
        assert_eq!(
            capture.0,
            [
                (
                    crate::vterm_defs::VTermAttr::Bold,
                    crate::vterm_defs::VTermValue::Boolean(1),
                ),
                (
                    crate::vterm_defs::VTermAttr::Font,
                    crate::vterm_defs::VTermValue::Number(4),
                ),
                (
                    crate::vterm_defs::VTermAttr::Foreground,
                    crate::vterm_defs::VTermValue::Color(color),
                ),
            ]
        );
    }

    #[test]
    fn set_pen_col_ansi_targets_foreground_or_background_and_emits() {
        let mut state = VTermPenState::default();
        let mut capture = PenCapture::default();
        set_pen_col_ansi(
            &mut state,
            &mut capture,
            crate::vterm_defs::VTermAttr::Foreground,
            5,
        );
        set_pen_col_ansi(
            &mut state,
            &mut capture,
            crate::vterm_defs::VTermAttr::Background,
            260,
        );
        assert!(state.pen.fg.is_indexed());
        assert_eq!(state.pen.fg.index, 5);
        assert!(state.pen.bg.is_indexed());
        assert_eq!(state.pen.bg.index, 4);
        assert_eq!(
            capture.0,
            [
                (
                    crate::vterm_defs::VTermAttr::Foreground,
                    crate::vterm_defs::VTermValue::Color(state.pen.fg),
                ),
                (
                    crate::vterm_defs::VTermAttr::Background,
                    crate::vterm_defs::VTermValue::Color(state.pen.bg),
                ),
            ]
        );
    }

    #[test]
    fn sgr_intensity_handles_bold_dim_and_normal_intensity() {
        let mut state = VTermPenState::default();
        let mut capture = PenCapture::default();
        assert!(apply_sgr_intensity(&mut state, &mut capture, 1));
        assert!(state.pen.bold);
        assert!(apply_sgr_intensity(&mut state, &mut capture, 2));
        assert!(state.pen.dim);
        assert!(apply_sgr_intensity(&mut state, &mut capture, 22));
        assert!(!state.pen.bold);
        assert!(!state.pen.dim);
        assert!(!apply_sgr_intensity(&mut state, &mut capture, 99));
    }

    #[test]
    fn sgr_bold_promotes_low_index_foreground_when_configured() {
        let mut state = VTermPenState {
            bold_is_highbright: true,
            ..Default::default()
        };
        crate::vterm_defs::vterm_color_indexed(&mut state.pen.fg, 3);
        let mut capture = PenCapture::default();
        assert!(apply_sgr_intensity(&mut state, &mut capture, 1));
        assert_eq!(state.pen.fg.index, 11);

        state.pen.bold = false;
        state.pen.fg.color_type |= crate::vterm_defs::VTERM_COLOR_DEFAULT_FG;
        state.pen.fg.index = 2;
        assert!(apply_sgr_intensity(&mut state, &mut capture, 1));
        assert_eq!(state.pen.fg.index, 2);
    }

    #[test]
    fn sgr_italic_handles_enable_and_disable() {
        let mut state = VTermPenState::default();
        let mut capture = PenCapture::default();
        assert!(apply_sgr_italic(&mut state, &mut capture, 3));
        assert!(state.pen.italic);
        assert!(apply_sgr_italic(&mut state, &mut capture, 23));
        assert!(!state.pen.italic);
        assert!(!apply_sgr_italic(&mut state, &mut capture, 4));
        assert_eq!(
            capture.0,
            [
                (
                    crate::vterm_defs::VTermAttr::Italic,
                    crate::vterm_defs::VTermValue::Boolean(1),
                ),
                (
                    crate::vterm_defs::VTermAttr::Italic,
                    crate::vterm_defs::VTermValue::Boolean(0),
                ),
            ]
        );
    }

    #[test]
    fn sgr_underline_handles_single_double_curly_and_off() {
        let mut state = VTermPenState::default();
        let mut capture = PenCapture::default();
        assert_eq!(
            apply_sgr_underline(&mut state, &mut capture, &[4], 0),
            Some(1)
        );
        assert_eq!(state.pen.underline, crate::vterm_defs::VTERM_UNDERLINE_SINGLE);
        assert_eq!(
            apply_sgr_underline(&mut state, &mut capture, &[21], 0),
            Some(1)
        );
        assert_eq!(state.pen.underline, crate::vterm_defs::VTERM_UNDERLINE_DOUBLE);
        assert_eq!(
            apply_sgr_underline(&mut state, &mut capture, &[24], 0),
            Some(1)
        );
        assert_eq!(state.pen.underline, crate::vterm_defs::VTERM_UNDERLINE_OFF);
        assert_eq!(apply_sgr_underline(&mut state, &mut capture, &[5], 0), None);
    }

    #[test]
    fn sgr_underline_consumes_colon_subparameter() {
        let mut state = VTermPenState::default();
        let mut capture = PenCapture::default();
        for (subparameter, expected) in [
            (0, crate::vterm_defs::VTERM_UNDERLINE_OFF),
            (1, crate::vterm_defs::VTERM_UNDERLINE_SINGLE),
            (2, crate::vterm_defs::VTERM_UNDERLINE_DOUBLE),
            (3, crate::vterm_defs::VTERM_UNDERLINE_CURLY),
        ] {
            assert_eq!(
                apply_sgr_underline(
                    &mut state,
                    &mut capture,
                    &[crate::vterm::parser::CSI_ARG_FLAG_MORE | 4, subparameter],
                    0,
                ),
                Some(2)
            );
            assert_eq!(state.pen.underline, expected);
        }
    }

    #[test]
    fn sgr_visibility_toggles_blink_reverse_conceal_and_strike() {
            let mut state = VTermPenState::default();
            let mut capture = PenCapture::default();
            for (on, off, field) in [
                (5, 25, crate::vterm_defs::VTermAttr::Blink),
                (7, 27, crate::vterm_defs::VTermAttr::Reverse),
                (8, 28, crate::vterm_defs::VTermAttr::Conceal),
                (9, 29, crate::vterm_defs::VTermAttr::Strike),
            ] {
                assert!(apply_sgr_visibility(&mut state, &mut capture, on));
                assert_eq!(capture.0.last().unwrap().0, field);
                assert_eq!(
                    capture.0.last().unwrap().1,
                    crate::vterm_defs::VTermValue::Boolean(1)
                );
                assert!(apply_sgr_visibility(&mut state, &mut capture, off));
                assert_eq!(
                    capture.0.last().unwrap().1,
                    crate::vterm_defs::VTermValue::Boolean(0)
                );
            }
            assert!(!state.pen.blink);
            assert!(!state.pen.reverse);
            assert!(!state.pen.conceal);
            assert!(!state.pen.strike);
            assert!(!apply_sgr_visibility(&mut state, &mut capture, 6));
    }

    #[test]
    fn sgr_font_selects_all_ten_font_slots() {
        let mut state = VTermPenState::default();
        let mut capture = PenCapture::default();
        for argument in 10..=19 {
            assert!(apply_sgr_font(&mut state, &mut capture, argument));
            assert_eq!(state.pen.font, (argument - 10) as u8);
        }
        assert!(!apply_sgr_font(&mut state, &mut capture, 9));
        assert!(!apply_sgr_font(&mut state, &mut capture, 20));
        assert_eq!(
            capture.0.last(),
            Some(&(
                crate::vterm_defs::VTermAttr::Font,
                crate::vterm_defs::VTermValue::Number(9),
            ))
        );
    }

    #[test]
    fn sgr_overline_handles_enable_and_disable() {
        let mut state = VTermPenState::default();
        let mut capture = PenCapture::default();
        assert!(apply_sgr_overline(&mut state, &mut capture, 53));
        assert!(state.pen.overline);
        assert!(apply_sgr_overline(&mut state, &mut capture, 55));
        assert!(!state.pen.overline);
        assert!(!apply_sgr_overline(&mut state, &mut capture, 54));
        assert_eq!(
            capture.0,
            [
                (
                    crate::vterm_defs::VTermAttr::Overline,
                    crate::vterm_defs::VTermValue::Boolean(1),
                ),
                (
                    crate::vterm_defs::VTermAttr::Overline,
                    crate::vterm_defs::VTermValue::Boolean(0),
                ),
            ]
        );
    }

    #[test]
    fn sgr_baseline_handles_super_subscript_and_normal() {
        let mut state = VTermPenState::default();
        let mut capture = PenCapture::default();
        for (argument, small, baseline) in [
            (73, true, crate::vterm_defs::VTERM_BASELINE_RAISE),
            (74, true, crate::vterm_defs::VTERM_BASELINE_LOWER),
            (75, false, crate::vterm_defs::VTERM_BASELINE_NORMAL),
        ] {
            assert!(apply_sgr_baseline(
                &mut state,
                &mut capture,
                argument,
            ));
            assert_eq!(state.pen.small, small);
            assert_eq!(state.pen.baseline, baseline);
        }
        assert!(!apply_sgr_baseline(&mut state, &mut capture, 72));
        assert_eq!(capture.0.len(), 6);
    }

    #[test]
    fn sgr_standard_color_maps_foreground_and_background_ranges() {
        let mut state = VTermPenState::default();
        let mut capture = PenCapture::default();
        for argument in 30..=37 {
            assert!(apply_sgr_standard_color(
                &mut state,
                &mut capture,
                argument,
            ));
            assert_eq!(state.pen.fg.index, (argument - 30) as u8);
        }
        for argument in 40..=47 {
            assert!(apply_sgr_standard_color(
                &mut state,
                &mut capture,
                argument,
            ));
            assert_eq!(state.pen.bg.index, (argument - 40) as u8);
        }
        assert!(!apply_sgr_standard_color(&mut state, &mut capture, 29));
        assert!(!apply_sgr_standard_color(&mut state, &mut capture, 48));
    }

    #[test]
    fn sgr_standard_foreground_honors_bold_highbright_mode() {
        let mut state = VTermPenState {
            bold_is_highbright: true,
            ..Default::default()
        };
        state.pen.bold = true;
        let mut capture = PenCapture::default();
        assert!(apply_sgr_standard_color(&mut state, &mut capture, 32));
        assert_eq!(state.pen.fg.index, 10);
        assert!(apply_sgr_standard_color(&mut state, &mut capture, 42));
        assert_eq!(state.pen.bg.index, 2);
    }

    #[test]
    fn sgr_high_color_maps_foreground_and_background_ranges() {
        let mut state = VTermPenState::default();
        let mut capture = PenCapture::default();
        for argument in 90..=97 {
            assert!(apply_sgr_high_color(&mut state, &mut capture, argument));
            assert_eq!(state.pen.fg.index, (argument - 90 + 8) as u8);
        }
        for argument in 100..=107 {
            assert!(apply_sgr_high_color(&mut state, &mut capture, argument));
            assert_eq!(state.pen.bg.index, (argument - 100 + 8) as u8);
        }
        assert!(!apply_sgr_high_color(&mut state, &mut capture, 89));
        assert!(!apply_sgr_high_color(&mut state, &mut capture, 108));
    }

    #[test]
    fn sgr_default_color_restores_foreground_and_background() {
        let mut state = VTermPenState::default();
        vterm_state_newpen(&mut state);
        crate::vterm_defs::vterm_color_indexed(&mut state.pen.fg, 7);
        crate::vterm_defs::vterm_color_indexed(&mut state.pen.bg, 8);
        let mut capture = PenCapture::default();
        assert!(apply_sgr_default_color(&mut state, &mut capture, 39));
        assert_eq!(state.pen.fg, state.default_fg);
        assert!(apply_sgr_default_color(&mut state, &mut capture, 49));
        assert_eq!(state.pen.bg, state.default_bg);
        assert!(!apply_sgr_default_color(&mut state, &mut capture, 40));
    }

    #[test]
    fn sgr_alternate_color_parses_rgb_foreground_and_indexed_background() {
        let mut state = VTermPenState::default();
        let mut capture = PenCapture::default();
        assert_eq!(
            apply_sgr_alternate_color(&mut state, &mut capture, &[38, 2, 1, 2, 3], 0),
            SgrColorResult::Applied(5)
        );
        assert_eq!((state.pen.fg.red, state.pen.fg.green, state.pen.fg.blue), (1, 2, 3));
        assert_eq!(
            apply_sgr_alternate_color(&mut state, &mut capture, &[48, 5, 200], 0),
            SgrColorResult::Applied(3)
        );
        assert!(state.pen.bg.is_indexed());
        assert_eq!(state.pen.bg.index, 200);
    }

    #[test]
    fn sgr_alternate_color_reports_unhandled_and_truncated_inputs() {
        let mut state = VTermPenState::default();
        let mut capture = PenCapture::default();
        assert_eq!(
            apply_sgr_alternate_color(&mut state, &mut capture, &[37], 0),
            SgrColorResult::Unhandled
        );
        assert_eq!(
            apply_sgr_alternate_color(&mut state, &mut capture, &[38], 0),
            SgrColorResult::Truncated
        );
        assert!(capture.0.is_empty());
    }

    #[test]
    fn setpen_applies_mixed_sgr_sequence_in_order() {
        let mut state = VTermPenState::default();
        vterm_state_newpen(&mut state);
        let mut capture = PenCapture::default();
        vterm_state_setpen(
            &mut state,
            &[
                1,
                3,
                crate::vterm::parser::CSI_ARG_FLAG_MORE | 4,
                3,
                5,
                7,
                8,
                9,
                14,
                31,
                44,
                53,
                73,
            ],
            &mut capture,
        );
        assert!(state.pen.bold);
        assert!(state.pen.italic);
        assert_eq!(state.pen.underline, crate::vterm_defs::VTERM_UNDERLINE_CURLY);
        assert!(state.pen.blink);
        assert!(state.pen.reverse);
        assert!(state.pen.conceal);
        assert!(state.pen.strike);
        assert_eq!(state.pen.font, 4);
        assert_eq!(state.pen.fg.index, 1);
        assert_eq!(state.pen.bg.index, 4);
        assert!(state.pen.overline);
        assert!(state.pen.small);
        assert_eq!(state.pen.baseline, crate::vterm_defs::VTERM_BASELINE_RAISE);
    }

    #[test]
    fn setpen_handles_reset_extended_colors_and_truncated_sequence() {
        let mut state = VTermPenState::default();
        vterm_state_newpen(&mut state);
        let mut capture = PenCapture::default();
        vterm_state_setpen(
            &mut state,
            &[38, 2, 10, 20, 30, 48, 5, 200],
            &mut capture,
        );
        assert_eq!((state.pen.fg.red, state.pen.fg.green, state.pen.fg.blue), (10, 20, 30));
        assert_eq!(state.pen.bg.index, 200);

        vterm_state_setpen(&mut state, &[crate::vterm::parser::CSI_ARG_MISSING], &mut capture);
        assert_eq!(state.pen.fg, state.default_fg);
        assert_eq!(state.pen.bg, state.default_bg);

        let before = state.clone();
        vterm_state_setpen(&mut state, &[38], &mut capture);
        assert_eq!(state, before);
    }

    #[test]
    fn setpen_skips_unknown_colon_subparameters() {
        let mut state = VTermPenState::default();
        let mut capture = PenCapture::default();
        vterm_state_setpen(
            &mut state,
            &[
                crate::vterm::parser::CSI_ARG_FLAG_MORE | 999,
                123,
                2,
            ],
            &mut capture,
        );
        assert!(state.pen.dim);
    }

    #[test]
    fn append_pen_color_serializes_indexed_color_ranges() {
        for (index, foreground, expected) in [
            (3, true, vec![33]),
            (3, false, vec![43]),
            (11, true, vec![93]),
            (11, false, vec![103]),
            (
                200,
                true,
                vec![
                    crate::vterm::parser::CSI_ARG_FLAG_MORE | 38,
                    crate::vterm::parser::CSI_ARG_FLAG_MORE | 5,
                    200,
                ],
            ),
        ] {
            let mut color = crate::vterm_defs::VTermColor::default();
            crate::vterm_defs::vterm_color_indexed(&mut color, index);
            let mut args = Vec::new();
            append_pen_color(&color, foreground, &mut args);
            assert_eq!(args, expected);
        }
    }

    #[test]
    fn append_pen_color_serializes_rgb_and_omits_matching_defaults() {
        let mut color = crate::vterm_defs::VTermColor::default();
        crate::vterm_defs::vterm_color_rgb(&mut color, 1, 2, 3);
        let mut args = Vec::new();
        append_pen_color(&color, true, &mut args);
        assert_eq!(
            args,
            [
                crate::vterm::parser::CSI_ARG_FLAG_MORE | 38,
                crate::vterm::parser::CSI_ARG_FLAG_MORE | 2,
                crate::vterm::parser::CSI_ARG_FLAG_MORE | 1,
                crate::vterm::parser::CSI_ARG_FLAG_MORE | 2,
                3,
            ]
        );
        color.color_type |= crate::vterm_defs::VTERM_COLOR_DEFAULT_FG;
        args.clear();
        append_pen_color(&color, true, &mut args);
        assert!(args.is_empty());
    }

    #[test]
    fn getpen_serializes_all_noncolor_attributes_in_source_order() {
        let state = VTermPenState {
            pen: VTermPen {
                bold: true,
                dim: true,
                italic: true,
                underline: crate::vterm_defs::VTERM_UNDERLINE_CURLY,
                blink: true,
                reverse: true,
                conceal: true,
                strike: true,
                font: 4,
                overline: true,
                small: true,
                baseline: crate::vterm_defs::VTERM_BASELINE_RAISE,
                fg: crate::vterm_defs::VTermColor {
                    color_type: crate::vterm_defs::VTERM_COLOR_DEFAULT_FG,
                    ..Default::default()
                },
                bg: crate::vterm_defs::VTermColor {
                    color_type: crate::vterm_defs::VTERM_COLOR_DEFAULT_BG,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            vterm_state_getpen(&state),
            [
                1,
                2,
                3,
                crate::vterm::parser::CSI_ARG_FLAG_MORE | 4,
                3,
                5,
                7,
                8,
                9,
                14,
                53,
                73,
            ]
        );
    }

    #[test]
    fn getpen_serializes_double_underline_and_both_colors() {
        let mut state = VTermPenState::default();
        state.pen.underline = crate::vterm_defs::VTERM_UNDERLINE_DOUBLE;
        crate::vterm_defs::vterm_color_indexed(&mut state.pen.fg, 2);
        crate::vterm_defs::vterm_color_rgb(&mut state.pen.bg, 10, 20, 30);
        assert_eq!(
            vterm_state_getpen(&state),
            [
                21,
                32,
                crate::vterm::parser::CSI_ARG_FLAG_MORE | 48,
                crate::vterm::parser::CSI_ARG_FLAG_MORE | 2,
                crate::vterm::parser::CSI_ARG_FLAG_MORE | 10,
                crate::vterm::parser::CSI_ARG_FLAG_MORE | 20,
                30,
            ]
        );
    }

    #[test]
    fn set_penattr_updates_typed_scalar_and_color_fields() {
        let mut state = VTermPenState::default();
        let mut capture = PenCapture::default();
        let color = crate::vterm_defs::VTermColor {
            red: 1,
            green: 2,
            blue: 3,
            ..Default::default()
        };
        let cases = [
            (
                crate::vterm_defs::VTermAttr::Bold,
                crate::vterm_defs::VTermValue::Boolean(2),
            ),
            (
                crate::vterm_defs::VTermAttr::Underline,
                crate::vterm_defs::VTermValue::Number(6),
            ),
            (
                crate::vterm_defs::VTermAttr::Font,
                crate::vterm_defs::VTermValue::Number(18),
            ),
            (
                crate::vterm_defs::VTermAttr::Foreground,
                crate::vterm_defs::VTermValue::Color(color),
            ),
            (
                crate::vterm_defs::VTermAttr::Uri,
                crate::vterm_defs::VTermValue::Number(42),
            ),
        ];
        for (attr, value) in &cases {
            assert_eq!(
                vterm_state_set_penattr(
                    &mut state,
                    *attr,
                    value.value_type(),
                    Some(value),
                    &mut capture,
                ),
                1
            );
        }
        assert!(state.pen.bold);
        assert_eq!(state.pen.underline, 2);
        assert_eq!(state.pen.font, 2);
        assert_eq!(state.pen.fg, color);
        assert_eq!(state.pen.uri, 42);
        assert_eq!(capture.0.len(), cases.len());
    }

    #[test]
    fn set_penattr_rejects_null_wrong_type_and_nonattributes() {
        let mut state = VTermPenState::default();
        let original = state.clone();
        let mut capture = PenCapture::default();
        assert_eq!(
            vterm_state_set_penattr(
                &mut state,
                crate::vterm_defs::VTermAttr::Bold,
                crate::vterm_defs::VTermValueType::Bool,
                None,
                &mut capture,
            ),
            0
        );
        let number = crate::vterm_defs::VTermValue::Number(1);
        assert_eq!(
            vterm_state_set_penattr(
                &mut state,
                crate::vterm_defs::VTermAttr::Bold,
                crate::vterm_defs::VTermValueType::Int,
                Some(&number),
                &mut capture,
            ),
            0
        );
        assert_eq!(
            vterm_state_set_penattr(
                &mut state,
                crate::vterm_defs::VTermAttr::NAttrs,
                crate::vterm_defs::VTermValueType::None,
                Some(&number),
                &mut capture,
            ),
            0
        );
        assert_eq!(state, original);
        assert!(capture.0.is_empty());
    }

    #[test]
    fn ansi_color_table_matches_pen_c() {
        assert_eq!(ANSI_COLORS.len(), 16);
        assert_eq!(ANSI_COLORS[0], VTermRgb { red: 0, green: 0, blue: 0 });
        assert_eq!(ANSI_COLORS[7], VTermRgb { red: 224, green: 224, blue: 224 });
        assert_eq!(ANSI_COLORS[8], VTermRgb { red: 128, green: 128, blue: 128 });
        assert_eq!(ANSI_COLORS[15], VTermRgb { red: 255, green: 255, blue: 255 });
    }

    #[test]
    fn terminal_pen_defaults_to_zeroed_c_state() {
        let pen = VTermPen::default();
        assert_eq!(pen.fg, crate::vterm_defs::VTermColor::default());
        assert_eq!(pen.bg, crate::vterm_defs::VTermColor::default());
        assert_eq!(pen.uri, 0);
        assert!(!pen.bold);
        assert_eq!(pen.underline, 0);
        assert!(!pen.italic);
        assert!(!pen.blink);
        assert!(!pen.reverse);
        assert!(!pen.conceal);
        assert!(!pen.strike);
        assert_eq!(pen.font, 0);
        assert!(!pen.small);
        assert_eq!(pen.baseline, 0);
        assert!(!pen.dim);
        assert!(!pen.overline);
    }

    #[test]
    fn color_ramps_match_pen_c() {
        assert_eq!(RAMP6, [0x00, 0x33, 0x66, 0x99, 0xCC, 0xFF]);
        assert_eq!(RAMP24.first(), Some(&0x00));
        assert_eq!(RAMP24.last(), Some(&0xFF));
        assert_eq!(RAMP24.len(), 24);
        assert!(RAMP24.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn default_ansi_lookup_maps_all_sixteen_colors() {
        for (index, expected) in ANSI_COLORS.iter().enumerate() {
            let mut color = crate::vterm_defs::VTermColor::default();
            lookup_default_colour_ansi(index as i64, &mut color);
            assert!(color.is_rgb());
            assert_eq!(
                (color.red, color.green, color.blue),
                (expected.red, expected.green, expected.blue)
            );
        }
    }

    #[test]
    fn default_ansi_lookup_leaves_invalid_indices_unchanged() {
        let original = crate::vterm_defs::VTermColor {
            color_type: crate::vterm_defs::VTERM_COLOR_INDEXED,
            index: 42,
            ..Default::default()
        };
        for index in [-1, 16, i64::MAX] {
            let mut color = original;
            lookup_default_colour_ansi(index, &mut color);
            assert_eq!(color, original);
        }
    }

    #[test]
    fn terminal_palette_defaults_to_the_ansi_table() {
            let palette = VTermPalette::default();
            for (color, expected) in palette.colors.iter().zip(ANSI_COLORS) {
                assert_eq!(
                    (color.red, color.green, color.blue),
                    (expected.red, expected.green, expected.blue)
                );
            }
    }

    #[test]
    fn pen_state_defaults_to_zeroed_allocator_storage() {
                let state = VTermPenState::default();
                assert_eq!(state.pen, VTermPen::default());
                assert_eq!(state.saved_pen, VTermPen::default());
                assert_eq!(
                    state.default_fg,
                    crate::vterm_defs::VTermColor::default()
                );
                assert_eq!(
                    state.default_bg,
                    crate::vterm_defs::VTermColor::default()
                );
                assert!(
                    state
                        .palette
                        .colors
                        .iter()
                        .all(|color| *color == crate::vterm_defs::VTermColor::default())
                );
                assert!(!state.bold_is_highbright);
    }

    #[test]
    fn set_default_colors_replaces_metadata_with_the_matching_default_flag() {
        let mut state = VTermPenState::default();
        let fg = crate::vterm_defs::VTermColor {
            color_type: crate::vterm_defs::VTERM_COLOR_INDEXED
                | crate::vterm_defs::VTERM_COLOR_DEFAULT_BG,
            index: 7,
            ..Default::default()
        };
        let bg = crate::vterm_defs::VTermColor {
            color_type: crate::vterm_defs::VTERM_COLOR_RGB
                | crate::vterm_defs::VTERM_COLOR_DEFAULT_FG,
            red: 1,
            green: 2,
            blue: 3,
            ..Default::default()
        };
        vterm_state_set_default_colors(&mut state, Some(&fg), Some(&bg));
        assert!(state.default_fg.is_indexed());
        assert!(state.default_fg.is_default_fg());
        assert!(!state.default_fg.is_default_bg());
        assert_eq!(state.default_fg.index, 7);
        assert!(state.default_bg.is_rgb());
        assert!(!state.default_bg.is_default_fg());
        assert!(state.default_bg.is_default_bg());
        assert_eq!(
            (
                state.default_bg.red,
                state.default_bg.green,
                state.default_bg.blue,
            ),
            (1, 2, 3)
        );
    }

    #[test]
    fn set_default_colors_leaves_none_side_unchanged() {
        let mut state = VTermPenState::default();
        state.default_fg.red = 9;
        state.default_bg.blue = 8;
        let original = state.clone();
        vterm_state_set_default_colors(&mut state, None, None);
        assert_eq!(state, original);
    }

    #[test]
    fn newpen_initializes_defaults_and_all_ansi_colors() {
        let mut state = VTermPenState::default();
        vterm_state_newpen(&mut state);
        assert_eq!(
            (
                state.default_fg.red,
                state.default_fg.green,
                state.default_fg.blue,
            ),
            (240, 240, 240)
        );
        assert!(state.default_fg.is_default_fg());
        assert_eq!(
            (
                state.default_bg.red,
                state.default_bg.green,
                state.default_bg.blue,
            ),
            (0, 0, 0)
        );
        assert!(state.default_bg.is_default_bg());
        for (color, expected) in state.palette.colors.iter().zip(ANSI_COLORS) {
            assert_eq!(
                (color.red, color.green, color.blue),
                (expected.red, expected.green, expected.blue)
            );
            assert_eq!(color.color_type, crate::vterm_defs::VTERM_COLOR_RGB);
        }
    }

    #[test]
    fn newpen_does_not_modify_current_or_saved_pen() {
        let mut state = VTermPenState::default();
        state.pen.bold = true;
        state.saved_pen.italic = true;
        vterm_state_newpen(&mut state);
        assert!(state.pen.bold);
        assert!(state.saved_pen.italic);
    }

    #[test]
    fn resetpen_clears_attributes_and_restores_default_colors() {
        let mut state = VTermPenState::default();
        vterm_state_newpen(&mut state);
        state.pen = VTermPen {
            bold: true,
            underline: 3,
            italic: true,
            blink: true,
            reverse: true,
            conceal: true,
            strike: true,
            font: 9,
            small: true,
            baseline: 2,
            dim: true,
            overline: true,
            uri: 42,
            ..Default::default()
        };
        let mut capture = PenCapture::default();
        vterm_state_resetpen(&mut state, &mut capture);
        assert_eq!(
            state.pen,
            VTermPen {
                fg: state.default_fg,
                bg: state.default_bg,
                ..Default::default()
            }
        );
        assert_eq!(
            capture
                .0
                .iter()
                .map(|(attr, _)| *attr)
                .collect::<Vec<_>>(),
            [
                crate::vterm_defs::VTermAttr::Bold,
                crate::vterm_defs::VTermAttr::Underline,
                crate::vterm_defs::VTermAttr::Italic,
                crate::vterm_defs::VTermAttr::Blink,
                crate::vterm_defs::VTermAttr::Reverse,
                crate::vterm_defs::VTermAttr::Conceal,
                crate::vterm_defs::VTermAttr::Strike,
                crate::vterm_defs::VTermAttr::Font,
                crate::vterm_defs::VTermAttr::Small,
                crate::vterm_defs::VTermAttr::Baseline,
                crate::vterm_defs::VTermAttr::Dim,
                crate::vterm_defs::VTermAttr::Overline,
                crate::vterm_defs::VTermAttr::Foreground,
                crate::vterm_defs::VTermAttr::Background,
                crate::vterm_defs::VTermAttr::Uri,
            ]
        );
    }

    #[test]
    fn savepen_copies_without_emitting_and_restore_emits_every_attribute() {
        let mut state = VTermPenState {
            pen: VTermPen {
                bold: true,
                underline: 2,
                italic: true,
                font: 4,
                uri: 9,
                ..Default::default()
            },
            ..Default::default()
        };
        let saved = state.pen;
        let mut capture = PenCapture::default();
        vterm_state_savepen(&mut state, 1, &mut capture);
        assert_eq!(state.saved_pen, saved);
        assert!(capture.0.is_empty());

        state.pen = VTermPen::default();
        vterm_state_savepen(&mut state, 0, &mut capture);
        assert_eq!(state.pen, saved);
        assert_eq!(capture.0.len(), 15);
        assert_eq!(
            capture.0.last(),
            Some(&(
                crate::vterm_defs::VTermAttr::Uri,
                crate::vterm_defs::VTermValue::Number(9),
            ))
        );
    }

    #[test]
    fn set_palette_color_replaces_valid_entries() {
        let mut state = VTermPenState::default();
        vterm_state_newpen(&mut state);
        let replacement = crate::vterm_defs::VTermColor {
            color_type: crate::vterm_defs::VTERM_COLOR_INDEXED
                | crate::vterm_defs::VTERM_COLOR_DEFAULT_FG,
            index: 99,
            ..Default::default()
        };
        vterm_state_set_palette_color(&mut state, 7, &replacement);
        assert_eq!(state.palette.colors[7], replacement);
    }

    #[test]
    fn set_palette_color_ignores_out_of_range_indices() {
        let mut state = VTermPenState::default();
        vterm_state_newpen(&mut state);
        let original = state.palette.clone();
        let replacement = crate::vterm_defs::VTermColor {
            red: 1,
            green: 2,
            blue: 3,
            ..Default::default()
        };
        for index in [-1, 16, i32::MAX] {
            vterm_state_set_palette_color(&mut state, index, &replacement);
        }
        assert_eq!(state.palette, original);
    }

    #[test]
    fn convert_color_to_rgb_uses_mutable_palette_and_clears_metadata() {
            let mut state = VTermPenState::default();
            vterm_state_newpen(&mut state);
            let replacement = crate::vterm_defs::VTermColor {
                red: 1,
                green: 2,
                blue: 3,
                ..Default::default()
            };
            vterm_state_set_palette_color(&mut state, 5, &replacement);
            let mut color = crate::vterm_defs::VTermColor {
                color_type: crate::vterm_defs::VTERM_COLOR_INDEXED
                    | crate::vterm_defs::VTERM_COLOR_DEFAULT_FG,
                index: 5,
                ..Default::default()
            };
            vterm_state_convert_color_to_rgb(&state, &mut color);
            assert!(color.is_rgb());
            assert!(!color.is_default_fg());
            assert!(!color.is_default_bg());
            assert_eq!((color.red, color.green, color.blue), (1, 2, 3));
        }

    #[test]
    fn convert_color_to_rgb_preserves_rgb_payload_while_clearing_metadata() {
            let state = VTermPenState::default();
            let mut color = crate::vterm_defs::VTermColor {
                color_type: crate::vterm_defs::VTERM_COLOR_RGB
                    | crate::vterm_defs::VTERM_COLOR_DEFAULT_BG,
                red: 10,
                green: 20,
                blue: 30,
                ..Default::default()
            };
            vterm_state_convert_color_to_rgb(&state, &mut color);
            assert_eq!(color.color_type, crate::vterm_defs::VTERM_COLOR_RGB);
            assert_eq!((color.red, color.green, color.blue), (10, 20, 30));
    }

    #[test]
    fn ansi_palette_lookup_copies_valid_entries_only() {
            let mut palette = VTermPalette::default();
            crate::vterm_defs::vterm_color_indexed(&mut palette.colors[3], 99);
            let mut color = crate::vterm_defs::VTermColor::default();
            assert!(lookup_colour_ansi(&palette, 3, &mut color));
            assert_eq!(color, palette.colors[3]);

            for index in [-1, 16] {
                let original = color;
                assert!(!lookup_colour_ansi(&palette, index, &mut color));
                assert_eq!(color, original);
            }
    }

    #[test]
    fn palette_lookup_uses_mutable_ansi_entries() {
            let mut palette = VTermPalette::default();
            crate::vterm_defs::vterm_color_rgb(&mut palette.colors[5], 1, 2, 3);
            let mut color = crate::vterm_defs::VTermColor::default();
            assert!(lookup_colour_palette(&palette, 5, &mut color));
            assert_eq!((color.red, color.green, color.blue), (1, 2, 3));
        }

    #[test]
    fn palette_lookup_maps_color_cube_boundaries() {
            let palette = VTermPalette::default();
            let mut color = crate::vterm_defs::VTermColor::default();
            for (index, expected) in [
                (16, (0x00, 0x00, 0x00)),
                (17, (0x00, 0x00, 0x33)),
                (21, (0x00, 0x00, 0xFF)),
                (22, (0x00, 0x33, 0x00)),
                (51, (0x00, 0xFF, 0xFF)),
                (196, (0xFF, 0x00, 0x00)),
                (231, (0xFF, 0xFF, 0xFF)),
            ] {
                assert!(lookup_colour_palette(&palette, index, &mut color));
                assert_eq!(
                    (color.red, color.green, color.blue),
                    expected,
                    "index {index}"
                );
            }
        }

    #[test]
    fn palette_lookup_maps_grayscale_and_rejects_invalid_indices() {
            let palette = VTermPalette::default();
            let mut color = crate::vterm_defs::VTermColor::default();
            for (index, expected) in [(232, 0x00), (233, 0x0B), (254, 0xF3), (255, 0xFF)] {
                assert!(lookup_colour_palette(&palette, index, &mut color));
                assert_eq!(
                    (color.red, color.green, color.blue),
                    (expected, expected, expected)
                );
            }
            let original = color;
            for index in [-1, 256, i64::MAX] {
                assert!(!lookup_colour_palette(&palette, index, &mut color));
                assert_eq!(color, original);
            }
    }

    #[test]
    fn lookup_colour_parses_rgb_payloads_and_partial_input() {
            let mut color = crate::vterm_defs::VTermColor::default();
            assert_eq!(lookup_colour(2, &[10, 20], &mut color), 2);
            assert_eq!(color, crate::vterm_defs::VTermColor::default());
            assert_eq!(
                lookup_colour(
                    2,
                    &[
                        crate::vterm::parser::CSI_ARG_FLAG_MORE | 10,
                        crate::vterm::parser::CSI_ARG_FLAG_MORE | 20,
                        300,
                    ],
                    &mut color,
                ),
                3
            );
            assert_eq!((color.red, color.green, color.blue), (10, 20, 44));
        }

    #[test]
    fn lookup_colour_parses_indexed_payloads_and_missing_input() {
            let mut color = crate::vterm_defs::VTermColor::default();
            assert_eq!(lookup_colour(5, &[], &mut color), 0);
            assert_eq!(
                lookup_colour(
                    5,
                    &[crate::vterm::parser::CSI_ARG_MISSING],
                    &mut color,
                ),
                1
            );
            assert_eq!(color, crate::vterm_defs::VTermColor::default());
            assert_eq!(lookup_colour(5, &[300], &mut color), 1);
            assert!(color.is_indexed());
            assert_eq!(color.index, 44);
        }

    #[test]
    fn lookup_colour_rejects_unknown_palette_modes() {
            let original = crate::vterm_defs::VTermColor {
                red: 1,
                green: 2,
                blue: 3,
                ..Default::default()
            };
            let mut color = original;
            assert_eq!(lookup_colour(7, &[1, 2, 3], &mut color), 0);
            assert_eq!(color, original);
    }
}
