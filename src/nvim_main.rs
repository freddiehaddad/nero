//! Translated from `src/nvim/main.c` (startup parser core).
//!
//! Full process startup remains coupled to event-loop, channel, UI,
//! command, and file-loading subsystems. [`get_number_arg`] is an
//! independent command-line parser used by that flow. [`usage`] and
//! [`print_mainerr`] provide startup's process-independent text output.

use std::io::Write;

/// Maximum number of `+`/`-c`/`--cmd` commands (`MAX_ARG_CMDS`).
pub const MAX_ARG_CMDS: usize = 10;
pub const WIN_HOR: i32 = 1;
pub const WIN_VER: i32 = 2;
pub const WIN_TABS: i32 = 3;
pub const EDIT_NONE: i32 = 0;
pub const EDIT_FILE: i32 = 1;
pub const EDIT_STDIN: i32 = 2;
pub const EDIT_TAG: i32 = 3;
pub const EDIT_QF: i32 = 4;
pub const ERR_ARG_MISSING: &str = "Argument missing after";
pub const ERR_OPT_GARBAGE: &str = "Garbage after option argument";
pub const ERR_OPT_UNKNOWN: &str = "Unknown option argument";
pub const ERR_TOO_MANY_ARGS: &str = "Too many edit arguments";
pub const ERR_EXTRA_CMD: &str =
    "Too many \"+command\", \"-c command\" or \"--cmd command\" arguments";

const USAGE_TEXT: &str = concat!(
    "Usage:\n",
    "  nvim [options] [file ...]\n",
    "\nOptions:\n",
    "  --cmd <cmd>           Execute <cmd> before any config\n",
    "  +<cmd>, -c <cmd>      Execute <cmd> after config and first file\n",
    "  -l <script> [args...] Execute Lua <script> (with optional args)\n",
    "  -S <session>          Source <session> after loading the first file\n",
    "  -s <scriptin>         Read Normal mode commands from <scriptin>\n",
    "  -u <config>           Use this config file\n",
    "\n",
    "  -d                    Diff mode\n",
    "  -es, -Es              Silent (batch) mode\n",
    "  -h, --help            Print this help message\n",
    "  -i <shada>            Use this shada file\n",
    "  -n                    No swap file, use memory only\n",
    "  -o[N]                 Open N windows (default: one per file)\n",
    "  -O[N]                 Open N vertical windows (default: one per file)\n",
    "  -p[N]                 Open N tab pages (default: one per file)\n",
    "  -R                    Read-only (view) mode\n",
    "  -v, --version         Print version information\n",
    "  -V[N][file]           Verbose [level][file]\n",
    "\n",
    "  --                    Only file names after this\n",
    "  --api-info            Write msgpack-encoded API metadata to stdout\n",
    "  --clean               \"Factory defaults\" (skip user config and plugins, shada)\n",
    "  --embed               Use stdin/stdout as a msgpack-rpc channel\n",
    "  --headless            Don't start a user interface\n",
    "  --listen <address>    Serve RPC API from this address\n",
    "  --remote[-subcommand] Execute commands remotely on a server\n",
    "  --server <address>    Connect to this Nvim server\n",
    "  --startuptime <file>  Write startup timing messages to <file>\n",
    "\nSee \":help startup-options\" for all options.\n",
);

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

/// Print the command-line help (`usage`).
pub fn usage() {
    print!("{USAGE_TEXT}");
}

fn format_mainerr(
    program: &[u8],
    message: &[u8],
    extra1: Option<&[u8]>,
    extra2: Option<&[u8]>,
) -> Vec<u8> {
    let program = &program[crate::path::path_tail(program)..];
    let mut output = Vec::new();
    output.extend_from_slice(program);
    output.extend_from_slice(b": ");
    output.extend_from_slice(message);
    for extra in [extra1, extra2].into_iter().flatten() {
        output.extend_from_slice(b": \"");
        output.extend_from_slice(extra);
        output.push(b'"');
    }
    output.extend_from_slice(b"\nMore info with \"");
    output.extend_from_slice(program);
    output.extend_from_slice(b" -h\"\n");
    output
}

/// Print a fatal startup argument error (`print_mainerr`).
///
/// `program` replaces the original file-static `argv0`, letting startup
/// pass its already-owned argument without introducing another global.
pub fn print_mainerr(
    program: &[u8],
    message: &[u8],
    extra1: Option<&[u8]>,
    extra2: Option<&[u8]>,
) -> std::io::Result<()> {
    std::io::stderr().write_all(&format_mainerr(
        program, message, extra1, extra2,
    ))
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
        assert_eq!([EDIT_NONE, EDIT_FILE, EDIT_STDIN, EDIT_TAG, EDIT_QF], [0, 1, 2, 3, 4]);
        assert_eq!(ERR_ARG_MISSING, "Argument missing after");
        assert_eq!(ERR_OPT_GARBAGE, "Garbage after option argument");
        assert_eq!(ERR_OPT_UNKNOWN, "Unknown option argument");
        assert_eq!(ERR_TOO_MANY_ARGS, "Too many edit arguments");
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

    #[test]
    fn usage_text_matches_main_c() {
        assert!(USAGE_TEXT.starts_with("Usage:\n  nvim [options] [file ...]\n"));
        assert!(USAGE_TEXT.contains(
            "  --clean               \"Factory defaults\" (skip user config and plugins, shada)\n"
        ));
        assert!(USAGE_TEXT.contains(
            "  --startuptime <file>  Write startup timing messages to <file>\n"
        ));
        assert!(USAGE_TEXT.ends_with("\nSee \":help startup-options\" for all options.\n"));
        assert_eq!(USAGE_TEXT.lines().count(), 34);
    }

    #[test]
    fn format_mainerr_uses_the_program_tail_and_optional_details() {
        assert_eq!(
            format_mainerr(
                b"bin/nvim",
                b"Unknown option argument",
                Some(b"--bogus"),
                Some(b"tail"),
            ),
            b"nvim: Unknown option argument: \"--bogus\": \"tail\"\n\
              More info with \"nvim -h\"\n"
        );
    }

    #[test]
    fn format_mainerr_omits_absent_details() {
        assert_eq!(
            format_mainerr(b"nvim", b"Too many edit arguments", None, None),
            b"nvim: Too many edit arguments\nMore info with \"nvim -h\"\n"
        );
    }
}
