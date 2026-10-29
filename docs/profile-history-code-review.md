# Edited history and smoothing review

The ce-simplify-code setup and three shipped rubrics ran inline on the two
runtime corrections, history probe/recorder, expanded harness, browser reader
and verification script. Delegation was not authorized. Independent reviewer
contexts: 0. Applied reuse/quality/efficiency findings: 0/0/0; skipped: 0.
No new unreachable elements were identified. No lint or typecheck is configured
for this JavaScript/JSX project.

- Expected behavior: commit a green Fill in advanced editing, then smooth with
  the selected controls while preserving the region's painted creation color.
- Invariant: geometry owns creation colors; coordinate initialization samples
  the live processing palette when anti-aliasing is enabled.
- What definitely did not happen: the source image, selected profile, palette
  search, segmentation labels, typed parameters and PRNG did not first diverge.
  Strict document assertions passed those fields before reaching contours.
- Bug class: a stage received the wrong color owner; input validation also
  rejected region IDs that the disabled native sampler never reads.
- Causal chain: Fill changes editor pixels; Next rebuilds regions; profile
  loading replaces the live palette; smoothing incorrectly overrode that palette
  with region creation colors before coordinate initialization.
- First unintended side effect: ImageDocument passed region creation colors as
  the sampler palette, changing initialized coordinates and final curves.
- Canonical source of truth: the completed document's live palette for sampling
  and retained per-region creation colors for contour geometry.
- Competing sources: the previous override supplied creation colors to both.
- Symptom: complete contours and fitted output differed despite matching earlier
  document fields. The original failing log remains retained.
- Trigger: paint green outside the fixed palette and continue to smoothing.
- Root cause: conflated color lifetimes, followed by unconditional palette bounds
  on the disabled anti-alias sampling path.
- Correct layer: ImageDocument's smoothing input and coordinate-initialization
  boundary, without changing tracing, optimization or expected native outputs.
- Minimal fix: remove the palette override; require live-palette bounds only when
  anti-alias sampling executes. Nonnegative integer color IDs remain required.
- Architectural follow-up: retain separate color ownership and broaden modified
  history/background tests. Do not clamp IDs or substitute a palette for an
  enabled sampler. Native out-of-bounds sampling remains an unresolved case.
- Patch: src/image-document.mjs and src/contour-initialization.mjs only.

The first proposed palette correction passed the new witness but exposed six
finite native editor cases with anti-aliasing disabled and IDs beyond the live
palette. The final boundary correction restores those cases. Frozen GUI 290/290,
installed direct 257/257, separate HTTP 6/6 and fresh installed CLI 19/19 pass. The new npm archive has
63 files and differs from the preceding package in exactly those two sources.

Three paired native history cases retain 19 complete documents and 55 history
observations. Together with the preceding guided corpus, the harness compares
154 complete documents across 20 available workflows, plus the separate hidden
Play-on-Export observation. History comparisons cover counts, serialized keys,
dimensions and exact opaque display pixels. Raw record payloads remain retained;
the GUI owner does not expose their individual binary fields for comparison.

The installed React GUI selects green, fills pixel 2,2, commits, runs its timer,
and returns to editing with Undo/Revert available. Its SVG is exactly the native
output. The real gesture uses -1 sentinels in unused Fill coordinate fields;
the prepared native command uses zeros. Browser comparisons explicitly do not
claim serialized stage-key equality across that admission difference. The engine
harness compares complete documents for the exact prepared commands separately.

The first native history setup ended one page too far; its page admission guard
rejected the edit before applying it. The failed request/response is retained.
The initial full-root run used the first palette-only candidate and was stopped
after the second correction. A later run started before snapshot copying ended;
its missing-file log was retained and the owned process tree stopped. The final
snapshot was hash-verified in full before launching the replacement root suite.
Neither incomplete run is acceptance evidence. The final full root run finishes
with 711 tests, 709 passing and two known failures; no tests are skipped or
cancelled. The alpha-premultiplied case now rejects its out-of-range live-palette
ID instead of returning finite coordinates different from native NaNs. Native's
undefined read remains unreproduced; the failing test stays visible. The other
failure remains uninitialized 16-bit Adam7 decoding. Full parity is unproven.
