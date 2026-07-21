//! Translated from `src/nvim/errors.h`.
//!
//! Shared error messages. Excludes errors only used once and debugging
//! messages (matches the original file's own comment).
//!
//! Original identifiers (`e_abort`, ...) are kept verbatim instead of
//! renaming to `SCREAMING_SNAKE_CASE`, for direct traceability back to the
//! C source; `non_upper_case_globals` is allowed module-wide for that reason.
//!
//! The original declares these as `EXTERN const char e_xxx[] INIT(= N_(...))`
//!   - the `EXTERN`/`INIT` trick lets every `.c` file that includes the
//!     header share one definition (real storage only in `main.c`). Rust
//!     `const`s don't need that trick at all: each `pub const` below is
//!     inlined at every use site, so a single definition here is already
//!     exactly equivalent.
//!
//! A handful of messages build their format string via C preprocessor/string
//! literal concatenation with `PRId64`/`PRIu64`/`PRIX64` (from
//! `<inttypes.h>`). Those macros expand to `"lld"`/`"llu"`/`"llX"` on the
//! Windows/UCRT target this was translated against; each such constant below
//! has a comment noting the original macro so the resolved literal can be
//! re-checked against a different target if needed.

#![allow(non_upper_case_globals)]

use crate::gettext_defs::gettext_noop as N_;

pub const e_abort: &str = N_("E470: Command aborted");
pub const e_afterinit: &str = N_("E905: Cannot set this option after startup");
pub const e_api_spawn_failed: &str = N_("E903: Could not spawn API job");
pub const e_argreq: &str = N_("E471: Argument required");
pub const e_backslash: &str = N_("E10: \\ should be followed by /, ? or &");
pub const e_cmdwin: &str = N_("E11: Invalid in command-line window; <CR> executes, CTRL-C quits");
pub const e_curdir: &str = N_("E12: Command not allowed in secure mode in current dir or tag search");
pub const e_invalid_buffer_name_str: &str = N_("E158: Invalid buffer name: %s");
pub const e_command_too_recursive: &str = N_("E169: Command too recursive");
pub const e_buffer_nr_invalid_buffer_number: &str = N_("E680: <buffer=%d>: invalid buffer number");
pub const e_buffer_is_not_loaded: &str = N_("E681: Buffer is not loaded");
pub const e_endif: &str = N_("E171: Missing :endif");
pub const e_endtry: &str = N_("E600: Missing :endtry");
pub const e_endwhile: &str = N_("E170: Missing :endwhile");
pub const e_endfor: &str = N_("E170: Missing :endfor");
pub const e_while: &str = N_("E588: :endwhile without :while");
pub const e_for: &str = N_("E588: :endfor without :for");
pub const e_exists: &str = N_("E13: File exists (add ! to override)");
pub const e_failed: &str = N_("E472: Command failed");
pub const e_intern2: &str = N_("E685: Internal error: %s");
pub const e_interr: &str = N_("Interrupted");
pub const e_invarg: &str = N_("E474: Invalid argument");
pub const e_invarg2: &str = N_("E475: Invalid argument: %s");
pub const e_invargval: &str = N_("E475: Invalid value for argument %s");
pub const e_invargNval: &str = N_("E475: Invalid value for argument %s: %s");
pub const e_duparg2: &str = N_("E983: Duplicate argument: %s");
pub const e_invexpr2: &str = N_("E15: Invalid expression: \"%s\"");
pub const e_invrange: &str = N_("E16: Invalid range");
pub const e_internal_error_in_regexp: &str = N_("E473: Internal error in regexp");
pub const e_invcmd: &str = N_("E476: Invalid command");
pub const e_isadir2: &str = N_("E17: \"%s\" is a directory");
pub const e_no_spell: &str = N_("E756: Spell checking is not possible");
pub const e_invchan: &str = N_("E900: Invalid channel id");
pub const e_invchanjob: &str = N_("E900: Invalid channel id: not a job");
pub const e_jobtblfull: &str = N_("E901: Job table is full");
pub const e_jobspawn: &str = N_("E903: Process failed to start: %s: \"%s\"");
pub const e_channotpty: &str = N_("E904: channel is not a pty");
pub const e_stdiochan2: &str = N_("E905: Couldn't open stdio channel: %s");
pub const e_invstream: &str = N_("E906: invalid stream for channel");
pub const e_invstreamrpc: &str = N_("E906: invalid stream for rpc channel, use 'rpc'");
/// `%" PRIu64` in the original.
pub const e_streamkey: &str = N_("E5210: dict key '%s' already set for buffered stream in channel %llu");
pub const e_libcall: &str = N_("E364: Library call failed for \"%s()\"");
pub const e_fsync: &str = N_("E667: Fsync failed: %s");
pub const e_mkdir: &str = N_("E739: Cannot create directory %s: %s");
pub const e_markinval: &str = N_("E19: Mark has invalid line number");
pub const e_marknotset: &str = N_("E20: Mark not set");
pub const e_modifiable: &str = N_("E21: Cannot make changes, 'modifiable' is off");
pub const e_nesting: &str = N_("E22: Scripts nested too deep");
pub const e_noalt: &str = N_("E23: No alternate file");
pub const e_noabbr: &str = N_("E24: No such abbreviation");
pub const e_nobang: &str = N_("E477: No ! allowed");
pub const e_nogroup: &str = N_("E28: No such highlight group name: %s");
pub const e_noinstext: &str = N_("E29: No inserted text yet");
pub const e_nolastcmd: &str = N_("E30: No previous command line");
pub const e_nomap: &str = N_("E31: No such mapping");
pub const e_noident: &str = N_("E349: No identifier under cursor");
pub const e_nomatch: &str = N_("E479: No match");
pub const e_nomatch2: &str = N_("E480: No match: %s");
pub const e_noname: &str = N_("E32: No file name");
pub const e_nopresub: &str = N_("E33: No previous substitute regular expression");
pub const e_noprev: &str = N_("E34: No previous command");
pub const e_noprevre: &str = N_("E35: No previous regular expression");
pub const e_norange: &str = N_("E481: No range allowed");
pub const e_noroom: &str = N_("E36: Not enough room");
pub const e_notmp: &str = N_("E483: Can't get temp file name");
pub const e_notopen: &str = N_("E484: Can't open file %s");
pub const e_notopen_2: &str = N_("E484: Can't open file %s: %s");
pub const e_cant_read_file_str: &str = N_("E485: Can't read file %s");
pub const e_null: &str = N_("E38: Null argument");
pub const e_number_exp: &str = N_("E39: Number expected");
pub const e_openerrf: &str = N_("E40: Can't open errorfile %s");
pub const e_outofmem: &str = N_("E41: Out of memory!");
pub const e_patnotf: &str = N_("Pattern not found");
pub const e_patnotf2: &str = N_("E486: Pattern not found: %s");
pub const e_positive: &str = N_("E487: Argument must be positive");
pub const e_prev_dir: &str = N_("E459: Cannot go back to previous directory");

pub const e_no_errors: &str = N_("E42: No Errors");
pub const e_loclist: &str = N_("E776: No location list");
pub const e_re_damg: &str = N_("E43: Damaged match string");
pub const e_re_corr: &str = N_("E44: Corrupted regexp program");
pub const e_readonly: &str = N_("E45: 'readonly' option is set (add ! to override)");
pub const e_letwrong: &str = N_("E734: Wrong variable type for %s=");
pub const e_illvar: &str = N_("E461: Illegal variable name: %s");
pub const e_cannot_mod: &str = N_("E995: Cannot modify existing variable");
pub const e_cannot_change_readonly_variable_str: &str =
    N_("E46: Cannot change read-only variable \"%.*s\"");
pub const e_dictreq: &str = N_("E715: Dictionary required");
/// `%" PRId64` in the original.
pub const e_blobidx: &str = N_("E979: Blob index out of range: %lld");
pub const e_invalblob: &str = N_("E978: Invalid operation for Blob");
pub const e_toomanyarg: &str = N_("E118: Too many arguments for function: %s");
pub const e_toofewarg: &str = N_("E119: Not enough arguments for function: %s");
pub const e_dictkey: &str = N_("E716: Key not present in Dictionary: \"%s\"");
pub const e_dictkey_len: &str = N_("E716: Key not present in Dictionary: \"%.*s\"");
pub const e_listreq: &str = N_("E714: List required");
pub const e_listblobreq: &str = N_("E897: List or Blob required");
pub const e_listblobarg: &str = N_("E899: Argument of %s must be a List or Blob");
pub const e_listdictarg: &str = N_("E712: Argument of %s must be a List or Dictionary");
pub const e_listdictblobarg: &str = N_("E896: Argument of %s must be a List, Dictionary or Blob");
pub const e_readerrf: &str = N_("E47: Error while reading errorfile");
pub const e_sandbox: &str = N_("E48: Not allowed in sandbox");
pub const e_secure: &str = N_("E523: Not allowed here");
pub const e_textlock: &str = N_("E565: Not allowed to change text or change window");
pub const e_screenmode: &str = N_("E359: Screen mode setting not supported");
pub const e_scroll: &str = N_("E49: Invalid scroll size");
pub const e_shellempty: &str = N_("E91: 'shell' option is empty");
pub const e_signdata: &str = N_("E255: Couldn't read in sign data!");
pub const e_swapclose: &str = N_("E72: Close error on swap file");
pub const e_command_too_complex: &str = N_("E74: Command too complex");
pub const e_longname: &str = N_("E75: Name too long");
pub const e_toomsbra: &str = N_("E76: Too many [");
pub const e_toomany: &str = N_("E77: Too many file names (glob not allowed)");
pub const e_trailing: &str = N_("E488: Trailing characters");
pub const e_trailing_arg: &str = N_("E488: Trailing characters: %s");
pub const e_umark: &str = N_("E78: Unknown mark");
pub const e_wildexpand: &str = N_("E79: Cannot expand wildcards");
pub const e_winheight: &str = N_("E591: 'winheight' cannot be smaller than 'winminheight'");
pub const e_winwidth: &str = N_("E592: 'winwidth' cannot be smaller than 'winminwidth'");
pub const e_write: &str = N_("E80: Error while writing");
pub const e_zerocount: &str = N_("E939: Positive count required");
pub const e_usingsid: &str = N_("E81: Using <SID> not in a script context");
pub const e_missingparen: &str = N_("E107: Missing parentheses: %s");
pub const e_empty_buffer: &str = N_("E749: Empty buffer");
/// `%" PRId64` in the original.
pub const e_nobufnr: &str = N_("E86: Buffer %lld does not exist");

pub const e_no_write_since_last_change: &str = N_("E37: No write since last change");
pub const e_no_write_since_last_change_add_bang_to_override: &str =
    N_("E37: No write since last change (add ! to override)");
pub const e_no_write_since_last_change_for_buffer_nr_add_bang_to_override: &str =
    N_("E89: No write since last change for buffer %d (add ! to override)");
pub const e_buffer_nr_not_found: &str = N_("E92: Buffer %d not found");
pub const e_unknown_function_str: &str = N_("E117: Unknown function: %s");
pub const e_str_not_inside_function: &str = N_("E193: %s not inside a function");
pub const e_job_still_running: &str = N_("E948: Job still running");
pub const e_job_still_running_add_bang_to_end_the_job: &str =
    N_("E948: Job still running (add ! to end the job)");

pub const e_invalpat: &str = N_("E682: Invalid search pattern or delimiter");
pub const e_bufloaded: &str = N_("E139: File is loaded in another buffer");
pub const e_notset: &str = N_("E764: Option '%s' is not set");
pub const e_invalidreg: &str = N_("E850: Invalid register name");
pub const e_dirnotf: &str = N_("E919: Directory not found in '%s': \"%s\"");
pub const e_au_recursive: &str = N_("E952: Autocommand caused recursive behavior");
pub const e_menu_only_exists_in_another_mode: &str = N_("E328: Menu only exists in another mode");
pub const e_autocmd_close: &str = N_("E813: Cannot close autocmd window");
/// `%" PRId64` in the original.
pub const e_list_index_out_of_range_nr: &str = N_("E684: List index out of range: %lld");
pub const e_listarg: &str = N_("E686: Argument of %s must be a List");
pub const e_unsupportedoption: &str = N_("E519: Option not supported");
pub const e_fnametoolong: &str = N_("E856: Filename too long");
pub const e_using_float_as_string: &str = N_("E806: Using a Float as a String");
pub const e_cannot_edit_other_buf: &str = N_("E788: Not allowed to edit another buffer now");
pub const e_using_number_as_bool_nr: &str = N_("E1023: Using a Number as a Bool: %d");
pub const e_not_callable_type_str: &str = N_("E1085: Not a callable type: %s");
pub const e_auabort: &str = N_("E855: Autocommands caused command to abort");

pub const e_api_error: &str = N_("E5555: API call: %s");

pub const e_fast_api_disabled: &str = N_("E5560: %s must not be called in a fast event context");
pub const e_noui: &str = N_("E5768: No UI attached");

pub const e_floatonly: &str = N_("E5601: Cannot close window, only floating window would remain");
pub const e_floatexchange: &str = N_("E5602: Cannot exchange or rotate float");

pub const e_cant_find_directory_str_in_cdpath: &str = N_("E344: Can't find directory \"%s\" in cdpath");
pub const e_cant_find_file_str_in_path: &str = N_("E345: Can't find file \"%s\" in path");
pub const e_no_more_directory_str_found_in_cdpath: &str =
    N_("E346: No more directory \"%s\" found in cdpath");
pub const e_no_more_file_str_found_in_path: &str = N_("E347: No more file \"%s\" found in path");

pub const e_value_is_locked: &str = N_("E741: Value is locked");
pub const e_value_is_locked_str: &str = N_("E741: Value is locked: %.*s");
pub const e_cannot_change_value: &str = N_("E742: Cannot change value");
pub const e_cannot_change_value_of_str: &str = N_("E742: Cannot change value of %.*s");
pub const e_cannot_set_variable_in_sandbox_str: &str =
    N_("E794: Cannot set variable in the sandbox: \"%.*s\"");
pub const e_cannot_delete_variable_str: &str = N_("E795: Cannot delete variable %.*s");
pub const e_invalwindow: &str = N_("E957: Invalid window number");
pub const e_problem_creating_internal_diff: &str = N_("E960: Problem creating the internal diff");

pub const e_cannot_define_autocommands_for_all_events: &str =
    N_("E1155: Cannot define autocommands for ALL events");
pub const e_cannot_change_arglist_recursively: &str =
    N_("E1156: Cannot change the argument list recursively");

pub const e_resulting_text_too_long: &str = N_("E1240: Resulting text too long");

pub const e_line_number_out_of_range: &str = N_("E1247: Line number out of range");

pub const e_highlight_group_name_invalid_char: &str = N_("E5248: Invalid character in group name");

pub const e_highlight_group_name_too_long: &str = N_("E1249: Highlight group name too long");

pub const e_string_required: &str = N_("E928: String required");

pub const e_invalid_column_number_nr: &str = N_("E964: Invalid column number: %ld");
pub const e_invalid_line_number_nr: &str = N_("E966: Invalid line number: %ld");

pub const e_reduce_of_an_empty_str_with_no_initial_value: &str =
    N_("E998: Reduce of an empty %s with no initial value");

/// The original is `N_("E1239: Invalid value for blob: 0x" PRIX64)` - note
/// there is no `%` before `PRIX64` in the upstream C source (unlike the
/// other `PRId64`/`PRIu64` usages above, which do have one). Preserved
/// verbatim, quirk and all, rather than "fixing" what looks like an
/// upstream oversight.
pub const e_invalid_value_for_blob_nr: &str = N_("E1239: Invalid value for blob: 0xllX");
pub const e_stray_closing_curly_str: &str = N_("E1278: Stray '}' without a matching '{': %s");
pub const e_missing_close_curly_str: &str = N_("E1279: Missing '}': %s");
pub const e_cannot_change_menus_while_listing: &str = N_("E1310: Cannot change menus while listing");
pub const e_not_allowed_to_change_window_layout_in_this_autocmd: &str =
    N_("E1312: Not allowed to change the window layout in this autocmd");

pub const e_val_too_large: &str = N_("E1510: Value too large: %s");
pub const e_val_too_large_len: &str = N_("E1510: Value too large: %.*s");

pub const e_undobang_cannot_redo_or_move_branch: &str =
    N_("E5767: Cannot use :undo! to redo or move to a different undo branch");

pub const e_winfixbuf_cannot_go_to_buffer: &str =
    N_("E1513: Cannot switch buffer. 'winfixbuf' is enabled");
pub const e_invalid_return_type_from_findfunc: &str =
    N_("E1514: 'findfunc' did not return a List type");
pub const e_cannot_switch_to_a_closing_buffer: &str = N_("E1546: Cannot switch to a closing buffer");
pub const e_cannot_have_more_than_nr_diff_anchors: &str =
    N_("E1549: Cannot have more than %d diff anchors");
pub const e_failed_to_find_all_diff_anchors: &str = N_("E1550: Failed to find all diff anchors");
pub const e_diff_anchors_with_hidden_windows: &str =
    N_("E1562: Diff anchors cannot be used with hidden diff windows");
pub const e_leadtab_requires_tab: &str =
    N_("E1572: 'listchars' field \"leadtab\" requires \"tab\" to be specified");
pub const e_invalid_format_string_single_percent_s: &str =
    N_("E1577: Invalid format string, only one \"%s\" is allowed");
pub const e_too_many_postponed_prefixes_spell: &str =
    N_("E1578: Too many postponed prefixes and/or compound flags");

pub const e_trustfile: &str = N_("E5570: Cannot update trust file: %s");
pub const e_cannot_read_from_str_2: &str = N_("E282: Cannot read from \"%s\"");

pub const e_conflicting_configs: &str = N_("E5422: Conflicting configs: \"%s\" \"%s\"");

pub const e_unknown_option2: &str = N_("E355: Unknown option: %s");

pub const e_restart_failed_cmd_no_quit: &str = N_("E5201: Restart failed: +cmd did not quit server: %s");

pub const top_bot_msg: &str = N_("search hit TOP, continuing at BOTTOM");
pub const bot_top_msg: &str = N_("search hit BOTTOM, continuing at TOP");

pub const line_msg: &str = N_(" line ");
