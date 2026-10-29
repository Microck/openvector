# Native mouse phases from browser button combinations

Every admitted mouse button transition must reach the selected editing tool
with its native press/release phase. The original press handlers test the held
left bit, rather than requiring left to be the changed button. Panning starts
on a changed left press and ends on any release in the pinned Qt 4.4.3 source.
The existing document gesture wire and tool implementations already express
these operations; the browser producer was dropping or misclassifying them.

The current Chromium capture establishes the first wrong effect. Right down
produces pointerdown, but adding left produces pointermove with button zero
and buttons three. Releasing left while right remains held produces another
pointermove. W3C Pointer Events 3 explicitly specifies these button semantics.
The pre-change browser misses Fill and the initial Pencil press, and leaves
Picker active until the final right release. These are actual trusted browser
samples, not constructed event inputs or a new native-input capture.

browserViewerPointer now converts the changed DOM button into the existing
Qt-style bit value, derives its phase from that bit and the full held mask,
and forwards the current modifiers. One Viewer callback handles all three
PointerEvent types. It dispatches intermediate presses/releases through the
existing queue and reserves movement for actual motion. Processing, editing
mode, layer independence, left-held tool admission and pointer capture retain
their existing owners. Pan checks the changed left button and clears its drag
on every release. No second mouse-event channel, historical adapter, new wire
shape or additional runtime validation was introduced.

The ce-simplify-code reuse, quality and efficiency passes ran inline with zero
independent reviewers, under the explicit no-delegation constraint. Applied
reuse 0, quality 1, efficiency 0; skipped 0. The quality pass replaced the nested
button-index conditional with a module-owned mapping table, avoiding per-event
array allocation. All 7,168 valid button/mask/modifier combinations match the
pre-simplification producer exactly. No additional simplification was justified.
The replaced inline producer and callbacks were removed as part of the single
canonical route; no residual unreachable product element was found. No lint
or typecheck script is configured in this JavaScript project. The known shell
launcher limitation was avoided by running the context script through Windows
Node; CE_CONTEXT_END was received once for this invocation.

The consolidate-test-suites invariant is correct phase/button metadata for
browser button combinations. Its primary unit owner is the existing viewer
input suite, which now consumes the five trusted PointerEvent records from
one small browser corpus and checks their complete native-style commands.
A second test owns middle/extra buttons and modifier preservation. The fixture
contains only browser inputs and provenance; native editor outputs are never
used as runtime inputs. The existing native keyboard and numerical viewer
comparisons remain intact. All 17 focused tests pass.

The browser probe owns a different failure mode: actual JSX callback binding,
world coordinates, capture and queued worker dispatch. Twenty-one observations
and 110 trusted mouse/pointer records cover right/left and middle/left input,
Fill, Pencil start/continuation, picker sampling before the last button release,
Zap, undo, right-only and middle-only guards, Pan start and intermediate-release
termination, and background toggles on additional presses while left is held.
No pointer or keyboard event is synthesized in this after-change browser check.
All recorded edit/background requests return 200 and the browser error list is
empty. The observer is removed, then both owned browsers and hosts close.
The post-Pan image shows selected readout text; browser text-selection policy
is still a separate unverified interaction, and no style change hides it here.

Probe dispositions: the browser capture/fixture, bounded verifier, before/after
observations, network records and screenshots are PROMOTE. The two producer
assertions are MERGE into the existing viewer-input suite. DROP: none. No new
native helper, compiler object or product instrumentation was created. Earlier
43 helpers and all 289 native/reference fixtures remain frozen; the new browser
fixture is copied normally into the isolated build rather than hard-linked.

The production build passes. The isolated GUI suite has 659 tests, 658 passing
and the same processing-batch-controls reset failure, with zero skips. Core
runtime files and all 64 existing core test files are unchanged from the prior
866-test run, whose encoded-PNG and edited-alpha failures remain visible.
The 123-file private npm package is unchanged; no repack or reinstall is needed
for this GUI correction. Original files, settings and user previews are preserved.

Full native button-combination capture, browser selection/focus behavior,
pen/touch devices and application-wide parity remain unverified. The captured
new sequence's final refit is not compared with a new original run. GitHub/npm
publication and the Rust/GPUI port remain unfinished.
