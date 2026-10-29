# Reduced working storage to full-source segmentation

The independent document API passes 16 additional native cancellation/retry
cases through both direct methods and runTask. The declared 512x400 four-color
source crosses the palette reduction threshold and produces a 438x342 working
image. Fixed/unlimited and aliased/antialiased profiles each have cold and warm
workflows, cancelled at the first or final segmentation checkpoint.

Warm workflows first complete full-source segmentation. Palette analysis then
replaces the live labels with a reduced map while keeping the old exported
region records. A changed quality forces the subsequent segmentation request
to do real work. Before its first (2,-1) callback the original has reallocated a
512x400 grid; labels are unavailable, and warm exported records remain. At
(6,-2), new full-size labels and exported records exist, but cancellation still
clears the completed stage key. Retry exactly matches the uninterrupted state.

No runtime implementation changed. The existing retained owner already keeps
dimensions, label availability, palette and exported records at their observed
producer boundaries. No large-image branch, label normalization, completed-map
facade or restoration shim was necessary.

## Evidence and ownership

The unchanged protocol-5 numeric observer ran 48 fresh processes. Two responses
for each request repeat byte for byte. The final auditor reparses every response,
verifies encoded requests and both frozen fixtures, checks cold/warm dimension
and record lifetimes, and preserves the preceding checkpoint evidence. Native
outputs are assertions only; declared pixels and controls supply independent
inputs. No original GUI input, widget setter or instruction patch was used.

The existing document-cancellation integration suite owns the two new fixture
families. It compares all observed progress tuples and exact double fractions,
source/working pixels, map bytes, exported records, committed settings, all 100
typed parameters, cache keys, candidates and random state, including first-chance
and post-unwind snapshots. The same suite already owns smaller cold/warm cases.
The installed harness owns a fresh consumer and checks real denials of original
and workspace reads and child creation. Earlier consumers remain untouched.

Applied ce-simplify-code's three full review rubrics inline, with zero independent
reviewers. Reuse 0, quality 0, efficiency 0, skipped 0. The capture reuses the
established numeric observer and parser; the audit reuses request/response and
test-total validation. No additional abstraction or simplification earned its
cost. No root lint/typecheck script is configured; JS and Python syntax checks
cover the changed test and evidence code.

Consolidate-test-suites disposition: MERGE the cases into the existing document
integration suite; PROMOTE the two fixtures and repeatable recorder/auditor.
The installed check owns package resolution and permission enforcement, a
separate boundary from native numerical state. No standalone regression suite
or new native helper was added. No application code became unreachable and no
deletion is pending.

## Verification boundary

The checkpoint records the full document and installed suite results after the
scoped cold/warm checks. All 205 application/source files remain unchanged.
All 125 files in the retained npm archive match source and the fresh install.
No second full numerical or GUI run is claimed for this test/evidence-only phase.
The original application/settings, 43 accepted helpers, five numeric candidates,
historical verification roots/archives, root gui/dist and existing previews are
preserved. No VCS metadata, commit, push or publication was created. JJ remains
unavailable.

Coverage includes first/final cancellation and complete progress traces, not
every intermediate large-image label map. Other dimensions, cached/interrupted
palette handoffs, old fitted-graph validity, alpha/edit/shadow contexts, original
scheduler timing and the three known failures remain open. Full parity,
publication and Rust/GPUI porting remain pending.
