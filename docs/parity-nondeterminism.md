# Historical Adam7 reference nondeterminism

This document records the 2026-10-20 native evidence and its test results.
The approved product behavior is defined in the
[image-input contract](image-input-contract.json) and
[profile-history contract](profile-history-contract.json).
Those contracts supersede the historical failing assertions described below.
The original captures remain unchanged.

## Finding

The retained native evidence proves that seven 16-bit Adam7 inputs do not have one repeatable whole-loader pixel result for the recorded arguments. All seven have different pixel results across the captured contexts. Thirteen of the 21 target/context pairs also differ between two fresh native processes with byte-identical requests.

This is a bounded observation about the original executable. It does not establish that every mismatch in an independent decoder is nondeterministic, and it does not make the existing strict fixture failures pass.

The 2026-10-20 analysis is in [analysis.json](../evidence/adam7-nondeterminism-20261020/analysis.json). It retains exact byte masks, expected stable bytes, observed variable values, request hashes and response hashes. [inputs.json](../evidence/adam7-nondeterminism-20261020/inputs.json) preserves the nine selected fixture inputs and their original expected stages. The seven targets and two prefix inputs come from `test/fixtures/image-input-reference.ndjson.gz`.

## Evidence and provenance

The original history audit and decoding diagnostic were absent from this Linux checkout. They were copied without modification from `windows-10-microck`, under `D:\migracion\vector-magic-re\evidence`, into the new evidence directory. The original Windows files, original probes and original installation were not modified. No native recapture was necessary because all 21 original requests and 42 original native responses were available.

| Retained artifact | SHA-256 |
| --- | --- |
| [History audit](../evidence/adam7-nondeterminism-20261020/retained/image-input-history-audit.json) | `ec56ba55c2436817271e09493cf55faa8938a12cc86c2008389be7c5e1431277` |
| [Decoding diagnostic](../evidence/adam7-nondeterminism-20261020/retained/image-input-legacy-decoding-diagnostic.json) | `172b468e9023e17c3bec674c8817f9ad1742b02bd60091c28c643fcb6376c24b` |
| Original compressed input fixture | `0559b95a44e14cc700b8f94f9487c43cb5c196d4b964dfdfb6c209de0166b34e` |

The copied JSON files and local fixture match hashes obtained from the Windows originals. The history audit identifies executable SHA-256 `c491656b962debc215a8b6e757a9b2949ffe50a36b5f2db692706bcd4ebface1` and capture-time probe SHA-256 `ba69258ac6313195b5df8dcebce40dd0303e7435fce42a01f4b956451c564224`. The latter identifies the historical capture, not the currently installed frozen probe.

The capture procedure is [`audit_context`](../tools/record-image-input.py#L223), lines 223 through 295. Each request selects operation 93 and supplies exact encoded image bytes, mode, amount and anti-alias control. It invokes the original encoded entry without patching original instructions. Each subprocess decodes either the target alone or one prefix followed by the target. The target is always the final response frame.

| Context | Prior input in the same process |
| --- | --- |
| `isolated` | None |
| `rgb-prefix` | `16x12-5-0-0`, RGB8 PNG |
| `rgba-prefix` | `16x12-10-0-0`, RGBA16 PNG |

Each target uses mode 0 and amount 0. The controls and encoded bytes are identical for that target across all three contexts. There are two fresh processes per context, giving six observations per target and 42 processes in total.

The analysis decoded the retained binary responses using the recorder's 1,220-byte header and stage layout. All 21 request files exactly match the selected fixture input bytes and controls. All 42 response hashes match the historical audit, every parsed target payload matches its audit payload, every per-frame status is 1, and parsing consumes the complete response. Raw request files, response files and original logs remain under [retained/](../evidence/adam7-nondeterminism-20261020/retained/), with their exact paths in `analysis.json.runs`.

## Concrete counterexamples

Offsets below are zero-based byte offsets into row-major BGRA, not pixel indices. The same observed differences occur in `source`, `working` and `prepared` for these captures.

For `adam7-9x7-10`, byte 5 is 0 in both isolated runs and both RGB-prefix runs. It is 20 and 6 in the two RGBA-prefix runs. Those last two processes have the same request bytes but different pixels. Their files are:

- [Request](../evidence/adam7-nondeterminism-20261020/retained/engine-image-input-context-adam7-9x7-10-rgba-prefix-requests.bin)
- [First native response](../evidence/adam7-nondeterminism-20261020/retained/engine-image-input-context-adam7-9x7-10-rgba-prefix-native.bin)
- [Second native response](../evidence/adam7-nondeterminism-20261020/retained/engine-image-input-context-adam7-9x7-10-rgba-prefix-repeat-native.bin)

For `adam7-9x7-26`, all three within-context response pairs are identical. Nevertheless, source byte 73 is 0 in both isolated and both RGB-prefix captures, but 6 in both RGBA-prefix captures. This case demonstrates the importance of changing prior image history even when one repeated request appears stable. It has two distinct target payloads across the six operation-93 observations. A passing pair of repeats is not a universal determinism guarantee.

For `adam7-17x13-6`, source byte 17 is 0/0 in the isolated pair, 65/103 in the RGB-prefix pair and 97/157 in the RGBA-prefix pair. The variability is in pixel payloads, not merely response metadata or addresses.

These observations establish dependence on state outside the target's recorded input and controls. The [existing image-input contract](image-input-contract.json) describes the recovered uninitialized interlace-row allocation and repeated in-place Q8 reduction. This audit does not independently re-establish that instruction-level cause. It establishes the output variation directly from native files and does not infer a particular allocator state from pixel values.

## Exact observed stable properties

Operation 93 exposes `source`, `working` and `prepared`. It does **not** expose the raw `decoded` stage. The following claims apply only to those three exposed stages and the specified observations.

Across all six operation-93 observations of each target:

- Width and height equal the dimensions in the sample ID. Every stage contains exactly `width * height * 4` pixel bytes.
- `source.matte` and `prepared.matte` are 1. `working` has no matte field in the native response; the audit represents it as `null`.
- For each individual observation, the complete BGRA arrays of `source`, `working` and `prepared` are identical. This claim is specific to these mode-0, amount-0 requests.
- Every alpha byte at offset `4 * pixel + 3` is stable across the six observations and the retained corpus stage. Stable does not mean opaque. In particular, even inputs written with opaque RGBA16 alpha retain non-255 alpha bytes after the native legacy processing.
- Every odd-numbered row is fully byte-stable across the six observations and the retained corpus stage. Some targets have additional fully stable rows, listed exactly in the analysis.

The table distinguishes the six whole-loader observations from those six plus the corresponding retained operation-92 corpus stage. The extra corpus observation narrows the stability claim for two targets.

| Exact sample ID | Input | Unique operation-93 payloads | Stable bytes in six operation-93 observations | Stable bytes including retained corpus |
| --- | --- | ---: | ---: | ---: |
| `adam7-9x7-10` | RGBA16, filter 0 | 4 | 246 / 252 | 246 / 252 |
| `adam7-9x7-25` | RGB16 with tRNS, filter 0 | 3 | 250 / 252 | 250 / 252 |
| `adam7-9x7-26` | Opaque RGBA16, filter 1 | 2 | 250 / 252 | 247 / 252 |
| `adam7-17x13-6` | RGB16, filter 1 | 6 | 847 / 884 | 844 / 884 |
| `adam7-17x13-10` | RGBA16, filter 0 | 6 | 865 / 884 | 865 / 884 |
| `adam7-17x13-25` | RGB16 with tRNS, filter 0 | 6 | 879 / 884 | 879 / 884 |
| `adam7-17x13-26` | Opaque RGBA16, filter 1 | 5 | 865 / 884 | 865 / 884 |

For example, row 1 of `adam7-9x7-26` has this exact stable BGRA hex in all three stages and all seven observations:

```text
553413ff9e7d5cffe7c6a5ff300feeff795837ffc2a180ff0beac9ff543312ff9d7c5bff
```

The analysis specifies every other stable byte, not just this example. For each `cases[]` entry and each stage, `operation93AndRetainedCorpus.stableByteMaskB64` contains one byte per output byte. A mask value of 255 means that `consensusBgraB64` gives the exact observed value. A mask value of 0 means that the observed value varied; the corresponding zero in the consensus array is a placeholder, not a native expectation. `unstableBytes` lists every variable offset, pixel coordinate, channel and observed value set. `completeStableRows` and `stableAlpha.valuesB64` contain exact expectations without any masked positions.

These masks describe a finite sample, not a proof that every other process history must preserve those bytes. A later native capture that contradicts a stable position must remain visible and extend the evidence. It must not be silently discarded.

## Raw-decoder failures at the audit

The historical diagnostic attempted all 337 cases and reports exactly the seven IDs above. It records 121 differing raw-decoded bytes in total but lists only 119 individual differences. `adam7-17x13-25` reports 42 differences and lists the first 40. The other six lists are complete.

Do not apply the source-stage variability mask to the raw `decoded` comparison. Source preparation can change or erase raw bytes. For example:

- `adam7-9x7-25` has historical raw-decoded mismatches at offsets 221 and 230, with independent value 0 and retained native value 201. Both corresponding source offsets are consistently 0 in this audit and its retained corpus source stage.
- `adam7-9x7-26` has a historical raw-decoded mismatch at offset 5, with independent value 0 and native value 1. Its corresponding source offset is consistently 0.

The operation-93 evidence does not prove those raw-decoded offsets variable. At
the audit, strict comparisons therefore remained enabled. Variability in one
stage did not justify masking a different byte, stage or sample.

The 2026-10-20 assertions compared complete buffers:

- [`test/image-input.test.mjs`](../test/image-input.test.mjs) compared the raw decoded buffer, source setter buffer, working buffer and prepared source buffer. Its first reported mismatch was `adam7-9x7-10`.
- [`rust/crates/openvector/tests/image_input.rs`](../rust/crates/openvector/tests/image_input.rs) kept `image::native_pixels` and `image::native_preparation` as full-byte fixture comparisons. Neither was ignored or converted into a masked comparison.

The retained-evidence audit did not execute the JavaScript or Rust implementation. Its historical diagnostic is separate from the subsequent Rust verification reported below.

## Historical stable-property tests

Seven tests added at the audit implemented the stable-property checks below in
`rust/crates/openvector/tests/image_input.rs`, using
`test/fixtures/adam7-stable-properties-20261020.json`. That fixture retains exact
per-stage masks and consensus bytes with the analysis SHA-256. These checks
supplemented the strict corpus tests at that time.

Each historical test used its encoded input and controls from the original fixture:

1. Assert decoded format `png`, dimensions, intrinsic alpha metadata from the retained raw fixture, input immutability and independent output ownership. These remain fixture assertions; operation 93 does not add repeated raw-stage evidence.
2. Run the ordinary independent source preparation. Assert all stage dimensions and exact stable bytes using that stage's `operation93AndRetainedCorpus` mask and consensus values. Compare every position marked 255, including zero-valued expectations.
3. Assert each complete stable row and every alpha byte against the exact recorded bytes. Do not replace alpha expectations with 255 for the opaque-input variants.
4. For these mode-0, amount-0 samples, assert complete equality between the independent source, working and prepared BGRA arrays. This checks the stage relationship even at positions whose exact native values varied.
5. Keep full native fixture assertions enabled until discrepancies are resolved or an explicit product decision defines behavior. The product decision now lives in the linked contracts. Captured scratch bytes and a conveniently selected native run are not valid decoder oracles.

Verification on 2026-10-20 in the isolated Paris Raft: all seven stable-property tests pass. The complete `image_input` target reports 119 passed, 2 failed, 0 ignored. The unchanged strict failures remain `adam7-9x7-10` decoded byte 5 (Rust 0, retained native 167) and source byte 5 (Rust 0, retained native 18). The two fresh-input tests also pass, comparing all 120 new cases. Stable-property success is not a claim of full byte parity.

# Alpha edit reference nondeterminism

## Finding and retained evidence

The original `alpha-premultiplied` editing workflow reads beyond its live palette during anti-aliased smoothing. Three retained fresh native processes produce different final geometry from the same request. The automatic result, every editor frame and the final non-geometry state agree. This does not resolve the independent engine's strict full-workflow failure.

The [alpha analysis](../evidence/native-edited-alpha-palette-sampler-confirmed/alpha-nondeterminism-20261020/analysis.json) records the common fields, original paths, exact file sizes and SHA-256 values. The [control verification](../evidence/native-edited-alpha-palette-sampler-confirmed/alpha-nondeterminism-20261020/controls-verification.json) and [sampler verification](../evidence/native-edited-alpha-palette-sampler-confirmed/alpha-nondeterminism-20261020/sampler-verification.json) are unmodified copies of the retained Windows evidence.

On 2026-10-20, the audit, both verification records, the request, all three raw responses and their original parsed JSON records were copied read-only from `windows-10-microck`, under `D:\migracion\vector-magic-re\evidence`. Each local file matches a SHA-256 calculated from the Windows original. The raw response and request hashes also match the historical verification. No original process was started, no native recapture was needed, and the original installation and settings were untouched.

The capture identifies executable SHA-256 `c491656b962debc215a8b6e757a9b2949ffe50a36b5f2db692706bcd4ebface1` and probe SHA-256 `81070161d7877d5798399a978f5f29a68b38ffefdf6c0954265f23b6054a76fa`. The [shared request](../evidence/native-edited-alpha-palette-sampler-confirmed/alpha-nondeterminism-20261020/requests.bin) has SHA-256 `45d2f72f82d6e3189d3e5537cc8df592d3063d297e94f453183d62818d956247`.

| Fresh original control | Response SHA-256 | Nonfinite node coordinates | Node 4 x, little-endian float64 bits |
| --- | --- | ---: | --- |
| [0](../evidence/native-edited-alpha-palette-sampler-confirmed/alpha-nondeterminism-20261020/control-0.bin) | `0913ef8b1c2d46e9b500f8379ce76cc70bb01127ff56e1c8f2068e4ff002011c` | 0 | `deadebfceaad0840` |
| [1](../evidence/native-edited-alpha-palette-sampler-confirmed/alpha-nondeterminism-20261020/control-1.bin) | `6074814f4ef6c3b368d01d28c00458a3122684f6bff60ec50cf325e5eb51191c` | 328 | `000000000000f87f` |
| [2](../evidence/native-edited-alpha-palette-sampler-confirmed/alpha-nondeterminism-20261020/control-2.bin) | `279548124b9b838b9ee3a3a5b6cd93728325496a496ad22f86111a0eb7fb72c7` | 0 | `c437bc9f53c90740` |

These controls ran without a debugger, instruction hooks or original-data writes. Their recorded settings fingerprints agree before and after, and all 43 guarded original files were unchanged. The hardware-observation capture is separate from these three controls.

## Action sequence and palette lifetime

The retained workflow loads the 32-by-32 `gui-auto-partial-alpha.png`, SHA-256 `42f54f02e8e2df8c9bfee3827ad08807a5fed243f4f1aa4a86f05d33eab6dec7`, and completes automatic processing. Its zero-based actions are:

1. Fill `(16,16)` with ARGB `[0.5,0.5,0,0]`.
2. Pencil from `(3.2,5.6)` to `(28.6,25.2)` with ARGB `[0.25,0.125,0.25,0.0625]`.
3. Undo, redo, revert, redo and redo.
4. Action 7 commits the editing history and starts smoothing and fitting.

Before action 7, the editor has 682 transparent pixels, 43 pixels with BGRA `[16,64,32,64]`, and 299 pixels with BGRA `[0,0,128,128]`. Native reconstruction creates seven regions with counts `[682,10,140,159,15,6,12]` and color IDs `[0,1,2,2,1,1,1]`. Color ID 2 represents painted red. It is not an extra transparency sentinel.

The reconstruction creates three colors. Loading the selected fixed-palette profile then replaces the live palette with its original two entries, transparent and BGRA `[160,80,20,255]`. Regions keep their creation colors and color IDs. Geometry uses those creation colors; coordinate initialization samples the live palette. Both the JavaScript and Rust document implementations preserve these distinct lifetimes.

The retained hardware observation follows the palette from three creation colors to two reloaded colors. In the direct-palette branch of sampler `0x4a2520`, fragment IDs are `2,0`. The palette begins at `0x982e070` and ends at `0x982e090`; `EDI` equals that end address. Original instruction `0x4a268d`, `SUBSS XMM2, dword ptr [EDI]`, reads beyond the palette. The observed instruction window matches the original executable. The debug process reads heap-guard bytes `abababababababab0000000000000000`.

This establishes the invalid native read. It does not identify the first arithmetic instruction that produces NaN in an unmodified control. The indirect accessor `0x488f60` was not the observed access path.

## Stable properties and separate coverage

All three controls match the historical fixture's complete automatic state and all nine editor frames. Their final source and working pixels, dimensions, segmentation labels and counts, region color IDs, selected settings, palettes, cache keys, random state and all 100 typed parameters agree. The region creation BGRA values also agree. Final contours, smoothing metadata, fitted geometry and output chains are outside this stable-property contract.

The random state is 4022 throughout the pre-refit editor frames and 3565 after native completion in all three controls. Settings and typed parameters agree before and after refitting. At the audit, the independent engine stopped at its palette-bound rejection and could not check the post-refit state.

The separately named `native_alpha_edit_stable_properties_before_refit` test in [`segmentation_editor.rs`](../rust/crates/openvector/tests/segmentation_editor.rs) uses [`alpha-nondeterminism-20261020.json`](../test/fixtures/alpha-nondeterminism-20261020.json). The fixture links to the analysis SHA-256 and all three response hashes. It contains only common expected values.

The test runs ordinary independent decoding and automatic processing, replays all seven pre-refit actions, and calls the real `commit_editing` operation. It compares the full automatic state, original pixels, editor pixels and histories, every committed segmentation label, region counts and color IDs, region creation colors, selected settings, source and working pixels, pre-refit random state and exact typed parameter bits. It stops at the commit boundary before undefined native sampling. Captured state, heap bytes and nonfinite geometry never become processing inputs.

At the audit, `alpha_edit_creation_palette_native_bits` separately compared
creation-palette float32 bits with the sampler capture.
`native_command_alpha_premultiplied` still failed during completion. No tests,
builds, linters or formatters ran for that evidence-only addition.

The approved product contract now requires a complete owned sampling palette and
valid edited output. It does not permit clamped color IDs, captured heap bytes,
injected NaNs or a selected native completion. See the linked contract and the
current verification results rather than treating these historical failures as
release exceptions.
