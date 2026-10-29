# Viewport selection and actual download verification

Panning or painting must not start browser text selection inside the image
viewport. The current pre-change build reproduces a selection containing
Segments and 2400% after a right/left Pan sequence. The same sequence on a
fresh compiled build selects no text after adding user-select:none solely
to .viewport. The image ends at exactly the same bounds, and the opacity field
outside that viewport still accepts its value through the worker. No inline
style, Selection API clearing, new pointer handler or focus workaround is used.

This is a single mechanical stylesheet change. Compiled JavaScript assets are
byte-identical to the previous verified build. The existing button-combination
browser verifier is copied without modification into the isolated root and
passes all 21 observations and 110 trusted mouse/pointer records again. Its
new outputs use that separate root and a new evidence prefix; historical
captures are preserved. No new style-only unit test is justified.

The broader GUI inspection also revisits the recorded browser-download gap.
A new bounded verifier retains the intended six vector and four bitmap formats,
checks actual completed files against their attachment bytes and independently
checks EPS's current-clock header. It fails at the first SVG completion gate:
Download was canceled. A separate PNG-path attempt receives the same driver
cancellation. A correctly configured fresh session, with the documented
--download-path on open and every command, still fails at SVG. Zero files are
saved, so no format is marked as a completed browser download. The remaining
eight formats and post-download comparisons are not executed or claimed.

The observed SVG request receives a 200 attachment response. A direct PNG
request still yields valid PNG bytes, and the GUI remains loaded. The owned
Chrome policy page has no DownloadRestrictions entry; an unrelated extension
allowlist does not establish a download prohibition. A network-tracker read
after the bitmap attempt also ends with a socket timeout. The exact cancellation
cause remains unresolved. No product transport, browser policy, security setting
or download decision is changed to bypass this observation. All owned browser
sessions, diagnostic tabs and both owned hosts close normally.

The ce-simplify-code review covers the new verification code; the product
change itself is mechanical. Reuse, quality and efficiency passes ran inline,
with zero independent reviewers under the explicit no-delegation rule. No
further changes were justified: applied 0/0/0, skipped 0. The diagnostic draft's
unused helper/import were removed before final review. No residual unreachable
product code was found. The context script completed through the already
verified Windows Node path with CE_CONTEXT_END. No lint/typecheck script is
configured. The Python verifier parses, while its browser gate remains failing.

The consolidate-test-suites invariant is selection-free viewport dragging.
The owning layer is browser end-to-end: a computed style alone cannot prove
the default selection stopped. The before/after real-input observation and
existing complete mouse sequence own this rule, rather than a duplicate
state-only assertion in the Node suite. The new export verifier owns a distinct
browser-to-filesystem boundary absent from the component suites. It stays an
explicitly failing diagnostic gate, not a passing substitute for export parity.

Probe dispositions: before/after selection records, screenshots, build identity,
reused browser observations and bounded download-failure evidence/verifier are
PROMOTE. MERGE and DROP: none. Existing reference helpers and captures are
preserved. No product instrumentation, new native helper or compiler object
was created. Only the 289 existing immutable reference fixtures use hard links;
the separate browser input fixture and all editable sources are ordinary copies.

The production build passes. Because all JavaScript/runtime and test sources
remain byte-identical, the prior GUI 658/659 result, 17 focused viewer tests
and core 864/866 result are reused within those scopes; they are not reported
as new full-suite runs. The three known failures remain. All 123 package files
remain unchanged. Native window/focus behavior, actual completed GUI downloads,
full application parity, publication and the Rust/GPUI port remain unfinished.
