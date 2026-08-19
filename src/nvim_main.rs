//! Translated from `src/nvim/main.c` (startup parser core).
//!
//! Full process startup remains coupled to event-loop, channel, UI,
//! command, and file-loading subsystems. [`get_number_arg`] is an
//! independent command-line parser used by that flow.

/// Maximum number of `+`/`-c`/`--cmd` commands (`MAX_ARG_CMDS`).
pub const MAX_ARG_CMDS: usize = 10;
pub const WIN_HOR: i32 = 1;
pub const WIN_VER: i32 = 2;
pub const WIN_TABS: i32 = 3;

/// Parameters shared by `main()` startup helpers (`mparm_T`).
#[derive(Debug, Clone, Default)]
pub struct Mparm {
    pub argv: Vec<Vec<u8>>,
    pub use_vimrc: Option<Vec<u8>>,
    pub clean: bool,
    pub n_commands: i32,
    pub commands: [Option<Vec<u8>>; MAX_ARG_CMDS],
    pub cmds_tofree: [bool; MAX_ARG_CMDS],
    pub n_pre_commands: i32,
    pub pre_commands: [Option<Vec<u8>>; MAX_ARG_CMDS],
    pub luaf: Option<Vec<u8>>,
    pub lua_arg0: i32,
    pub edit_type: i32,
    pub tagname: Option<Vec<u8>>,
    pub use_ef: Option<Vec<u8>>,
    pub input_istext: bool,
    pub no_swap_file: i32,
    pub use_debug_break_level: i32,
    pub window_count: i32,
    pub window_layout: i32,
    pub diff_mode: i32,
    pub listen_addr: Option<Vec<u8>>,
    pub remote: i32,
    pub server_addr: Option<Vec<u8>>,
    pub scriptin: Option<Vec<u8>>,
    pub scriptout: Option<Vec<u8>>,
    pub scriptout_append: bool,
    pub had_stdin_file: bool,
}

impl Mparm {
    #[must_use]
    pub fn argc(&self) -> i32 {
        self.argv.len() as i32
    }
}

/// Initialize startup parameters (`init_params`).
#[must_use]
pub fn init_params(argv: Vec<Vec<u8>>) -> Mparm {
    Mparm {
        argv,
        use_debug_break_level: -1,
        window_count: -1,
        lua_arg0: -1,
        ..Default::default()
    }
}

/// Select the default split direction for diff mode
/// (`set_window_layout`).
pub fn set_window_layout(params: &mut Mparm) {
    if params.diff_mode != 0 && params.window_layout == 0 {
        params.window_layout = if crate::diff::diffopt_horizontal() {
            WIN_HOR
        } else {
            WIN_VER
        };
    }
}

/// Parse a decimal number at `argument[*index]` (`get_number_arg`).
///
/// Leaves `default` and `index` unchanged when the next byte is not a
/// digit.
#[must_use]
pub fn get_number_arg(argument: &[u8], index: &mut usize, default: i32) -> i32 {
    if !argument
        .get(*index)
        .is_some_and(u8::is_ascii_digit)
    {
        return default;
    }
    let start = *index;
    while argument
        .get(*index)
        .is_some_and(u8::is_ascii_digit)
    {
        *index += 1;
    }
    std::str::from_utf8(&argument[start..*index])
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// UBSan runtime defaults (`__ubsan_default_options`).
#[must_use]
pub const fn ubsan_default_options() -> &'static str {
    "print_stacktrace=1"
}

/// ASan runtime defaults (`__asan_default_options`).
#[must_use]
pub const fn asan_default_options() -> &'static str {
    "handle_abort=1,handle_sigill=1"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_number_arg_parses_digits_and_advances_index() {
        let mut index = 2;
        assert_eq!(get_number_arg(b"-o120tail", &mut index, 1), 120);
        assert_eq!(index, 5);
    }

    #[test]
    fn get_number_arg_keeps_default_for_missing_or_nonnumeric_value() {
        let mut index = 2;
        assert_eq!(get_number_arg(b"-ox", &mut index, 7), 7);
        assert_eq!(index, 2);
        let mut end = 2;
        assert_eq!(get_number_arg(b"-o", &mut end, 9), 9);
        assert_eq!(end, 2);
    }

    #[test]
    fn sanitizer_defaults_match_main_c() {
        assert_eq!(ubsan_default_options(), "print_stacktrace=1");
        assert_eq!(
            asan_default_options(),
            "handle_abort=1,handle_sigill=1"
        );
    }

    #[test]
    fn mparm_default_has_exact_command_capacities() {
        let params = Mparm::default();
        assert_eq!(params.argc(), 0);
        assert_eq!(params.commands.len(), MAX_ARG_CMDS);
        assert_eq!(params.pre_commands.len(), MAX_ARG_CMDS);
        assert_eq!(params.cmds_tofree, [false; MAX_ARG_CMDS]);
    }

    #[test]
    fn init_params_sets_main_c_sentinel_defaults() {
        let params = init_params(vec![b"nvim".to_vec(), b"file".to_vec()]);
        assert_eq!(params.argc(), 2);
        assert_eq!(params.use_debug_break_level, -1);
        assert_eq!(params.window_count, -1);
        assert_eq!(params.lua_arg0, -1);
        assert_eq!(params.remote, 0);
    }

    #[test]
    fn set_window_layout_selects_diff_split_only_when_unspecified() {
        let mut params = Mparm {
            diff_mode: 1,
            ..Default::default()
        };
        set_window_layout(&mut params);
        assert!(matches!(params.window_layout, WIN_HOR | WIN_VER));
        let mut explicit = Mparm {
            diff_mode: 1,
            window_layout: WIN_TABS,
            ..Default::default()
        };
        set_window_layout(&mut explicit);
        assert_eq!(explicit.window_layout, WIN_TABS);
    }
}
