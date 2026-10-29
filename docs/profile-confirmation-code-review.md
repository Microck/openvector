# Pending overwrite warnings

The native modal loop continues to dispatch a previously armed Auto-Update timer.
A manual update can therefore wait underneath another warning. The previous React
operation held the engine queue while awaiting the first decision, so its timer
could not open the second warning. The browser baseline records that discrepancy.

The current queue serializes preflight and engine work. A preflight returns its
decision promise inside an object so the queue does not await the user. Accepted
work rejoins the same queue and reads the current controls. A synchronous stack
owns pending decisions, while React renders one shadcn dialog per entry. Only the
top entry can resolve. Each dialog focuses Cancel and restores its own opener.
Saved-profile overwrite uses the same stack and queue.

The ce-simplify-code rubrics were applied inline to the changed App, native
workflow harness, modal observers, recorder and browser evidence reader. Separate
reviewer contexts: 0. The explicit project prohibition on delegation takes
precedence over the skill's suggested dispatch. The Bash setup could not start
because this machine's Bash route requires unavailable WSL virtualization. The
same context script ran successfully with the installed Windows Node runtime.

- Reuse: 0 changes. The existing processing queue, AutoUpdate owner, palette
  owner, document comparison helper and shadcn dialogs remain canonical.
- Quality: 1 change, indentation of the accepted-update block. A comment explains
  why the stack needs a synchronous owner beside its React rendering snapshot.
  No newly unused runtime elements were found.
- Efficiency: 0 changes. Pending warnings retain only their own decision and
  opener. Popping releases that entry. Worker operations remain serialized;
  confirmation rendering adds no numerical pass or extra HTTP operation.

The final isolated GUI suite passes 306 tests, with no skips or cancellations.
Its four built frontend files match the browser-tested build byte for byte after
the comment/indentation change. No lint or typecheck script is configured in
either package; no such check is claimed.

Nine paired native cases provide 53 complete documents and eight modal
observations. The expanded workflow harness checks 36 available workflows and
274 complete documents, including separate pending document observations after
aborted actions. It does not count an interrupted final action as completed.
The native observers read modal metadata and model state, then abort without
answering, dismissing, changing focus or synthesizing native input. The delayed
variant observes natural timer dispatch for at least 750 ms before aborting.

The actual browser passes 19 UI checks and five native exposed-state comparisons.
Browser Fill gestures use -1 in unused coordinate fields, while the prepared
native commands use zero. The browser reader explicitly excludes the three
serialized keys that embed that different input form. It still checks exact
controls, palettes/scores, runtime ownership, editor pixels, history counts,
background state and SVG bytes. The full raw-document harness uses the exact
prepared native command form and compares all recorded fields.

Native after-answer behavior, keyboard dismissal semantics and full application
parity remain unverified. The numerical engine and packaged CLI did not change.
The last full root run remains historical: 711 tests, 709 passing and two
native-memory-dependent failures. This GUI-only change does not resolve either.
