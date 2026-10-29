# Reset Everything model evidence and list ownership

Five native workflows repeat complete response bytes across 18 fresh processes.
They distinguish the list reset at 0x42f380 from source/model reset at 0x430ed0
with three true arguments. The latter resets controls and the image/queue; it
cannot use the existing Start Over operation, which retains processing choices.
The pending warning confirms Yes/No with No explicitly default and focused.
The observer exits with code 19 without answering it. No post-confirmation
window handler is executed.

The independent changes implement the list stage: reset saved profiles to their
built-in entry, and clear recent files, icons and directories while preserving
the current source directory. The host persists those owners through their
existing atomic writers. HTTP comparisons cover a loaded image, rejected batch
requests, foreign origins, storage restart and real filesystem write failures.
The current image, selected controls, batch queue and export options survive
these list operations. No global reset button is exposed yet.

## Review

The ce-simplify-code reuse, quality and efficiency rubrics ran inline.
Independent review contexts: zero. This is one review context, not independent
corroboration. The unavailable Bash setup from the earlier session was not
retried; the same context script ran directly with the installed Node runtime.
The code graph still points to the old GuiEngine.reset location, so current
known files supplied the implementation evidence.

- Reuse: no additional review change. Profile mutations use one private writer;
  source reset-history uses the existing source mutation and error path.
- Quality: no additional change. Lists stay with their current owners. There
  is no second storage format, registry import, rollback or runtime reference
  dependency. No newly unused production elements were identified.
- Efficiency: one change. Reset constructs its single built-in entry directly,
  avoiding two allocations of the same default profile through put(). No
  startup scan, image decode, timer or additional hot-path validation was added.

## Verification and limits

All 334 GUI tests pass in the isolated verification copy, including seven new
reset checks. All 25 existing saved-profile and source-owner tests pass. No
lint/typecheck scripts are configured. The workspace GUI run passes 333 tests
and fails its branding assertion against the preserved old Ambivalence build.
That active preview and workspace gui/dist were not rebuilt.

The first new differential test incorrectly used the startup profile reader to
inspect live list order. Startup reinserts the default entry. The test now holds
the real list owner and compares its running state directly, with unchanged
native expectations. HTTP restart and persistence checks remain separate.

The first isolated build ran before the copy completed and its dependency link
existed. Its failure log is retained. The accepted build started after the copy
process returned exit zero. An installed-package check initially passed a URL
to the filename-only profile writer. Its failure log is retained; the accepted
check uses fileURLToPath and a fresh owned storage directory.

The packed package has 64 files and 19 entry points. Archive, workspace and
offline installation bytes match. The installed permission check compares every
built-in profile field with native bits, covers reset after eviction, owned
snapshots, repeated reset and an atomic storage round trip. Reads of the original
application and workspace module, plus child creation, are denied. This proves
the main process check only; worker permission inheritance is not claimed.

All 18 older accepted probe executables retain their hashes. Original files,
settings and input desktop remain unchanged. REA saves and closes a snapshot
with 37 evidence records. Two earlier interior-address queries returned other
containing functions; their raw responses remain retained and are not cited as
reset handlers. The corrected exact entries supply the reset analysis.

The full model reset, window widget defaults, after-answer behavior and complete
settings persistence remain unfinished. Two native documents were captured but
this change does not claim their complete numerical conformance. No GitHub or
npm publication, Rust port or full application parity is claimed.
