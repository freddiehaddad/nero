
## Session continuation update 110 (segment baseline `beef3dc`, 17/50)

Continued the standing "at least 50 commits" directive. Commits 8-17
of this segment (1-7 recorded in the prior checkpoint):

8.  `81ecf02` os/fs: `is_executable`. Unix checks the real exec bit via
    `access(X_OK)` but ONLY after a regular-file guard (a directory has
    search permission and must not be mistaken for a command); Windows
    has no exec bit, so upstream settles for "exists and is a regular
    file" - that difference is upstream's own. `X_OK` declared locally
    (libc omits it on Windows), matching the existing `R_OK`/`W_OK`
    precedent in the same file.
9.  `6de12d1` os/fs: `os_can_exe`/`is_executable_ext`/
    `is_executable_in_path`. `is_executable_ext` is Windows-only,
    mirroring the original's `#define`-to-`is_executable` elsewhere via
    a cfg-gated function pair. `os_buf` becomes a local `Vec<u8>`
    truncated back each round; `copy_option_part`'s `maxlen` still comes
    from `MAXPATHL` so the real truncation behaviour is kept.
10. `191d460` eval/fs: `executable()`/`exepath()`. Cross-verified
    against a real nvim first.
11. `f3eb6d9` eval/userfunc: `get_func_arity`. Its "blocked on
    `find_internal_func`" note was stale.
12. `fec48f6` normal: `nv_goto` ("G"/"gg"). A zero count means "no
    count given", NOT line zero - has its own test.
13. `d46481e` insert: `cursor_up_inner`/`cursor_down_inner`. The
    `while (n--)` loops must stay explicit, since their own bodies
    increment `n` when skipping a concealed line.
14. `332c2fe` insert: the `last_insert` family, making `"@."` real.
    `register.rs` had a placeholder that could never hold anything; it
    now delegates to `insert.rs`, which owns the state.
15. `4fc9fc4` insert: `ins_need_undo_get`, `get`/`set_can_cindent`,
    `buf_prompt_text`/`prompt_text`, `prompt_curpos_editable`,
    `undisplay_dollar`.
16. `fe0ae24` autocmd_defs: the `event_names` table (149 entries).
17. `e40421f` autocmd: `event_nr2name` now reads that table instead of
    the enum's `Debug` impl.

### Two findings worth carrying forward

**`K_SPECIAL` is `0x80`, so it collides with real UTF-8 bytes.** A test
asserted a multibyte character round-trips through `set_last_insert` as
its raw UTF-8; it failed. Probing `add_char2buf` directly showed the
implementation was right and the expectation wrong: U+4E00 encodes as
`E4 B8 80`, whose last byte IS `K_SPECIAL`, so it is correctly escaped
to a three-byte sequence to stay replayable through the typeahead
buffer. Any future test asserting on stored key/insert text must expect
this escaping for any byte equal to `0x80`.

**The sign in `event_names[]` is meaningful, not a round trip.** The
transcription script asserted the third field was uniformly negated; it
was not (65 negated, 84 not). `gen_events.lua` explains it: "Events
with positive keys aren't allowed in 'eventignorewin'", and
`get_event_name_no_group` tests `event <= 0` to walk that subset.
Modelled as an explicit `win_local` flag beside a plain un-negated
`event`. Had the assert not been there, the distinction would have been
silently dropped for all 65 window-local events - a good argument for
over-asserting in transcription cross-checks.

### Verification notes

Every commit passed the full battery (Windows + native Linux, debug +
release, clippy, doc, 20x flakiness both platforms). The battery caught
two regressions the tests alone did not: 4 `private_intra_doc_links`
warnings (fix is plain backticks, never a visibility bump) and 3
`doc_lazy_continuation` warnings (fix is rewording so a wrapped line
does not start with `-`).

One Linux flake (1/20) hit the known pre-existing `GC_FIRST_LIST`
triple (`get_func_tv_releases_a_list_argument_after_the_call`,
`multiple_lists_maintain_the_gc_linked_list_correctly`,
`evalvars_init_sets_real_startup_values`). Confirmed unrelated: the
commit was a static data table, and a 25x re-run was 0/25. NOTE: an
ad-hoc WSL loop reported 25/25 failures purely because `cargo` was not
on `PATH` - always `source ~/.cargo/env` first, and inspect actual
output before believing a uniform all-fail result.

`nvim --headless` HANGS on any script using `normal!` or `enew`, so
`nv_goto` was hand-traced instead. Simple `echo`-only scripts are fine.

### Confirmed blocked (do not re-investigate without a new angle)

`foldlevelExpr` (needs `eval0_simple_funccal` and a `w_p_script_ctx`
field); `trigger_cursorhold` (needs `did_cursorhold` and the typeahead
buffer); `event_name2nr`/`event_ignored` (need the generated
`event_hash` perfect hash); `win_comp_scroll` (needs `w_p_script_ctx`);
`free_register`/`int_cmp` (already documented as needing no Rust
equivalent); the ACL trio and `os_chown`/`os_fchown` (upstream no-ops
or Unix-only FFI with no translated caller).

### Next steps

Continue toward 50. Productive veins: `insert.c` still has
`del_char_after_col`/`ins_apply_autocmds`/`check_spell_redraw`;
`autocmd.c` has `aucmd_span_pattern`/`arg_augroup_get`;
`diff.c` has `clear_diffin`/`clear_diffout`/`diff_copy_entry`.
Re-verify each dependency directly - several "blocked" notes in this
codebase have turned out stale on inspection, including two this
segment.
