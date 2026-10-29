# Shared output choices

One GuiEngine owner holds layout, thin stroke, compact EPS and DXF mode. Batch
snapshots and the single-image form display owned projections. Batch processing
and the single-image GUI pass complete selected options to the existing writer
methods. Those per-call methods retain their established defaults and do not
read hidden application settings.

The native defaults are grouped cutouts, no thin stroke, normal EPS and dense
DXF lines. OpenVector previously started single-image layout at stacked shapes
and batch DXF at splines. Both application defaults now follow native state.
The compact EPS choice and DXF mode appear in both workspaces, with their
format-specific enabled states. The CLI exposes --compact and documents the
correct DXF default. Standalone serializer contracts remain unchanged.

The processing-preference file now stores outputOptions once at its root.
The former batch-local field is removed. Old local files fail with recovery
instructions; no migration, second store or compatibility behavior was added.
Model reset preserves these choices, matching native captures. Complete window
reset may have additional effects and remains unverified.

## Native evidence and implementation correction

A separate frozen observer reads MainModel fields 0x1d8 through 0x1e4 after
business-model actions. Eight paired workflows retain 36 processes, ten restart
phases and 30 snapshots. They cover each valid scalar value, repeated saves,
workspace changes, model reset and an ordinary queued-files restart. No native
widget setter, synthetic input or dialog answer is used. Earlier executables
remain unchanged.

The first implementation made GuiEngine.export read the application selection
implicitly. The broad GUI run retained 58 failures because that broke its
per-call contract and coupled serialization to hidden state. The correction
keeps the selected snapshot explicit in application calls and preserves the
existing writer. Existing native expectations and older test assertions were
not changed to make that run pass. A separate root GUI run retains its stale
Ambivalence build failure; workspace gui/dist was deliberately not rebuilt.

The final isolated build and GUI suite pass 376 of 377 tests. The existing
setter-only backgroundMode 2 comparison remains visible. Eleven new GUI tests
pass. The full core run passes 720 of 722 tests, with the earlier PNG16 Adam7
uninitialized-row comparison and alpha-edited out-of-range region-color failure
unchanged. That run began before the new compact CLI test was added; the
subsequent complete CLI-batch file passes all 43 tests. No numerical source file
changed while those long-running core cases executed.

## Browser and review

The browser verifies initial defaults, enabled/disabled format controls,
single-to-batch-to-single sharing, keyboard layout selection and a real compact
EPS download containing thin-stroke operators. Its fixture seeds one queue entry
before browser interaction; queue-adding UI is not claimed by this test. Restart
uses the same file without reseeding and records unchanged preference bytes at
startup. The restored controls match the pre-shutdown selection. Screenshots
retain the EPS, batch and final DXF views. Root background remains black and no
horizontal overflow appears. Both owned host processes and browser tabs close.
The downloaded bytes also match the verified EPS serializer applied to the
retained native fitted geometry with the observed creation date. The source
bitmap bytes are identical. This comparison does not claim a fresh native file
export capture for that browser interaction.

One check() helper reported failure while its click had already set the checkbox
and the host update temporarily disabled it. The next accessibility snapshot
showed Thin stroke checked. That helper error was not treated as evidence of a
failed state change or retried blindly. Both browser sessions have no console
warnings/errors. The downloaded file was preserved in the evidence directory.

Octen Design returned Screen Studio's export feature section. Its compact
format controls were viewed in a temporary browser tab. Existing OpenVector
components and black/white styling were retained; no marketing panel, new theme
or decorative animation was copied. Native model captures determine behavior.

The ce-simplify-code reuse, quality and efficiency rubrics ran inline with zero
independent contexts under the no-delegation rule. The Node setup script ran
directly because Bash is unavailable. Reuse removed duplicate output validation
at preference startup by using the existing configuration reader. Quality moved
the new build kind into the existing ordinary name map. Applied simplification
counts: reuse 1, quality 1, efficiency 0. No newly unreachable production symbol
was identified. There are no lint/typecheck scripts.

The package remains an offline-verifiable independent library/CLI. The original
application and workspace cannot be read by its permission-restricted main
verifier, which imports all 19 public entries and matches native output defaults.
This does not claim worker permission inheritance. Complete native quit order,
active-job shutdown, window owners, remaining parity failures, GitHub publication
and the Rust/GPUI port remain unfinished.
