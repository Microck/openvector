# Qt numeric reference review

Helper 35 measures owned QString conversions used by the recovered bitmap
size handlers. It never assigns text to a widget, invokes a widget setter,
synthesizes native input or answers a native dialog. Each input retains its
UTF-16 length, including embedded NULs. Integer/double success flags, suffix
removal, three binary64 results and two formatted strings cross the wire.
Raw bits preserve negative zero, subnormals and non-finite values.

The 91 inputs ran in three groups, each repeated in a fresh process. All six
responses reparse and their non-timing fields match. The fixture contains only
the conversion records; full request/response/proof files retain context.
All prior 34 executables, original installation files, saved settings and
the input desktop remain unchanged. The new executable is frozen at
1e152f089ee1e856ee8ca6610a400d4f6db0a39ab002a07917750eb09a2a23c1.

The first request exceeded the inherited 64-action admission limit and exited
13 before writing a frame. That request, empty response and log remain separate
diagnostic evidence. They are excluded from the accepted fixture. The recorder
now checks the limit and uses groups of at most 32; the executable did not
change after this diagnostic.

## Findings that affect implementation

The installed QtCore file version is 4.4.3.0. The pinned 4.4.3 source confirms
QString conversion tries the default locale, then C, with group separators
disallowed. This session accepts 1,5 as 1.5 and 1,000 as 1.0. The exact default
locale name is not established. Numeric formatting still emits decimal points.

Decimal and exponent syntax fails integer conversion. Overflow and nonzero
underflow rounded to zero fail double conversion. Special values have explicit
spellings. A percentage sign must be the final character for the widget's
suffix removal. The handlers ignore conversion success before their size
normalization. One-decimal formatting produces 1.25 -> 1.2 and 1.15 -> 1.1;
negative zero displays as 0.0. JavaScript coercion and formatting cannot define
this contract without comparisons.

## Review and verification

The consolidate-test-suites invariant is exact native conversion across the
typed ABI and wire boundary. The native reference integration layer owns it.
The immutable fixture and smallest scoped recorder are promoted for the future
numeric-control suite. No duplicate product tests, mocks or implementation
substitutes were added. This fixture does not prove an independent parser works.

All three ce-simplify-code rubrics ran inline with zero independent reviewers.
Reuse, quality and efficiency each applied zero simplifications. One low-value
symbol-cache optimization was skipped: adding cache state to this short-lived
reference process does not improve the product. No code became unreachable.

The C build passed -Wall -Wextra -Werror. Six captures and the full raw-response
audit pass; Python syntax and contract JSON checks pass. No lint/typecheck
scripts exist. Product tests were not rerun because product runtime files did
not change; their previous verification remains bounded by its recorded scope.

Locale ownership, further Unicode and rounding boundaries, exceptional size
normalization and independent GUI controls remain unfinished. Nothing was
published, and full application parity is unproven.
