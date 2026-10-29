# Independent numeric text review

gui/numeric-text.mjs supplies the numeric primitives needed by the recovered
bitmap size controls. The caller passes one session's locale symbols. The
module owns that context and performs the native default-locale/C sequence.
It returns value/ok results, including native zero-on-failure behavior.
Percentage conversion removes only a final literal percent sign.

The normalizer uses the complete measured Qt 4.4.3 QChar space inventory.
Locale symbol mapping precedes trailing-space handling, as the pinned
QLocalePrivate source requires. Group separators remain invalid numeric
tokens. Integer admission is strict decimal with signed-int32 bounds.
Floating admission preserves special spellings and negative zero, and rejects
overflow or nonzero underflow to zero.

Fixed-one-decimal formatting operates on the exact binary64 fraction with
BigInt and rounds ties to an even integer. This avoids changing native output
through a decimal-string intermediate. It preserves a nonzero negative
value's sign when rounding to zero; actual negative zero displays as 0.0.
NaN and infinities use the captured spellings.

## Evidence and test placement

New helper 36 reads owned default, system and C QLocale values, and classifies
every UTF-16 code unit using owned QChar values. The public 4.4.3 header and
original DLL instructions establish the four-byte QLocale value layout and
hidden return pointers. Two fresh processes agree on es_ES as default/system,
C numeric symbols, and all 65536 classifications. No global locale setter,
widget/model setter, input synthesis or dialog acknowledgement is used.
Earlier 35 executable hashes remain unchanged.

The consolidate-test-suites invariant is independent numeric conversion with
native values, bits and formatting. Unit coverage belongs to the canonical
gui/test suite. No existing numeric-text suite or equivalent parser was
available, so numeric-text.test.mjs is a component file inside that suite.
It checks all 91 native text records without supplying expected values as
computation input, plus exhaustive whitespace and snapshot ownership.
The native integration recorder separately owns ABI/wire validity. These
layers detect different failures. The fixture, recorder and unit suite are
promoted; no temporary product probes, mocks or duplicate suites remain.

## Simplification and checks

All three ce-simplify-code rubrics ran inline with zero independent reviewers.
Reuse and efficiency applied zero changes. Quality applied one clarification:
explicit parentheses group mixed AND/OR conditions without changing evaluation.
No findings were skipped and no existing code became unreachable.

The 93 unit tests pass with no failures or skips. The native C build passed
-Wall -Wextra -Werror; the locale/whitespace audit reparses both successful
responses. The earlier numeric audit still reparses all six responses and
retains its original fixture hash. JavaScript/Python syntax and contract JSON
checks pass. No lint or typecheck scripts are configured.

Existing GUI runtime files and the core/CLI package are unchanged. A full GUI
rerun was not needed for this new isolated primitive; its prior recorded
batch-control comparison failure remains unresolved.

## Remaining work

The host must select its locale context and the bitmap owner must use these
primitives for size commits, coupled dimension clamping and field display.
UI controls, persistence and direct bitmap saving remain pending. Current
native text evidence covers the captured es_ES default with C retry; broader
locale and rounding-boundary coverage remains necessary. The module does not
claim full application parity or a completed GUI feature. Nothing was published.
