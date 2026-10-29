# Active palette cancellation in batch processing

BatchRunner previously checked AbortSignal only after completed tasks. It now
forwards the document's existing palette checkpoints and asks that document to
cancel after its progress observer returns. The document throws its task exception
at the original checkpoint, preserving partial candidates, source and consumed
random state. One internal exception class distinguishes this signal from host
errors, including an ordinary Error named CancelTaskException. Numerical kernels,
checkpoint arithmetic, file formats and completed-task observer ordering stay intact.

The CLI worker polls its shared cancellation cell at each palette checkpoint.
Explicit host progress observers use the existing acknowledgement cell, shared
with stage observers, so cancellation and observer failures cannot race past the
checkpoint. Without a host progress observer, progress does not cross IPC.

The GUI polls cancellation even when the integer percentage repeats. It publishes
changed percentages without image payloads. Its explicit current.complete flag is
false during active work, including native 100% checkpoints, and true only at task
completion. Existing profile/Quick Save native traces continue to compare every
completed task and unchanged expected random state; their observers now select
completed events explicitly. No percentage or first-publication heuristic remains.
The palette readout shows Palette analysis: N% rather than announcing completion.
Tracking the active stage also prevents palette cancellation from taking the
cancelled-load reset branch and discarding the successfully loaded source.

The native handler at 0x431c40 ends at 0x431c94. Its source-reset call at 0x431c69
is reached for task 8, not palette task 2. Its complete assembly is retained.
The unchanged direct document suite still matches 234 native scenarios, including
exact checkpoint fractions, progress, PRNG, candidates and post-unwind state.
New batch integration checks compare complete retained state against that tested
document operation, cancel both before and after a completed candidate, then retry.
They do not claim a newly recorded whole native batch workflow.

Focused checks pass: initial document/batch/worker 23, expanded batch/image/worker/
GUI 56, and final GUI/profile/Quick Save 57. Fresh full runs report core 867/869
and GUI 659/660, zero skips, with exactly the previous three failures. The initial
isolated GUI run lacked three input images; those exact hashed inputs were copied
and its failed log retained. A second failed log records the missing event-phase
distinction before the complete flag fixed it. No reference expectation was changed.

The compiled browser displays 18% during actual palette work. Trusted Process and
Cancel clicks stop at 36%, with an incomplete palette, retained source, no outputs
and no browser errors. Both screenshots are retained. Both owned hosts and the
owned browser close; the user's preview and root gui/dist remain unchanged.

The private 123-file archive, source and fresh offline install match byte for byte.
Installed API cancellation/retry and CLI conversion pass with consumer-only file
permissions, actual denied original/workspace reads and denied child creation.
The batch worker CLI passes separately; Node worker permission inheritance is not
claimed. An accidental install in the parent directory was moved, after validating
its sole package/launchers/metadata, into the owned consumer's parent-install-mistake
folder. Automatic approval review blocked deletion, so cleanup used a reversible
move. No npm metadata remains in the parent directory; the correct install uses
an explicit consumer prefix. No GitHub or npm publication occurred.

ce-simplify-code covered the changed runtime, test and verification seams. The Bash
setup failed because WSL lacks /bin/bash; the same context script completed with
the known Windows Node runtime through CE_CONTEXT_END. Reuse, quality and efficiency
reviews ran inline under the no-delegation instruction: applied 0/0/0, skipped 0,
zero independent reviewers. No residual unreachable product elements were found.
No lint/typecheck script is configured; production build passes.

consolidate-test-suites places the state/retry invariant in the existing batch
integration suite, acknowledgement/error handling in the worker suite, and retained
source/repeated-percentage behavior in the GUI batch suite. Existing boundary
validation and native profile/Quick Save assertions are extended in place. No mocks
or new canonical test file were added. The installed diagnostic proves the package
boundary; the browser observations prove rendered controls. Their records, hashes,
screenshots and integrity auditor are PROMOTE. MERGE/DROP: none. No temporary product
instrumentation or native helper was created. Historical captures remain unchanged.

Other numerical tasks still require active checkpoint recovery. Export streaming,
actual completed browser downloads, the three known suite failures, full GUI parity,
publication and Rust/GPUI remain unfinished. The goal remains active.
