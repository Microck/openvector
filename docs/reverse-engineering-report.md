# Vector Magic engine recovery

The target is a 6,742,016-byte, 32-bit Windows executable from the bundled Vector Magic Desktop Edition 1.15 installation. Its SHA-256 is `c491656b962debc215a8b6e757a9b2949ffe50a36b5f2db692706bcd4ebface1`.

The intended product is a free, MIT-licensed vectorization engine, a terminal CLI, a custom GUI, and an npm package. The finished product must work without Vector Magic installed and must not bundle its executable or DLLs. The original serves as a development reference. Independent settings, weighted/unweighted fixed-count palette generation/refinement, palette assignment, unweighted segmentation with and without diagonal extraction, and flood fill now run through the library and CLI. Complete image conversion, the GUI, and full 1:1 behavior remain unfinished.

## Recovered material

| Material | Result | Local evidence |
| --- | --- | --- |
| Native overview | Ghidra identifies 12,614 procedures and 19,836 defined strings | `evidence/001-overview.json` |
| Byte-level string scan | 51,201 ASCII/UTF-16 candidate strings, including incidental binary matches | `evidence/strings.json` |
| Engine profiles | Ten complete profiles, each with 100 named numeric parameters and recovered storage types | `evidence/core-profiles.json`, `evidence/parameter-registry.json` |
| Embedded resources | 121 HTML pages, 259 PNGs, four translation catalogs, CSS, and an EULA | `evidence/qt-resource-index.json` |
| Native investigation | Entry points, settings translation, stage dispatch, and selected numerical helpers | `evidence/pseudocode/` and numbered REA records |
| Shipped examples | Four source images and 24 exports across six vector formats | `evidence/reference-corpus.json` |
| Original CPU recordings | 648 settings cases, 12,032 nearest-color decisions, 5,574 fills, 89 unweighted refinements, 44 peak scans, 74 unweighted generation runs, 471 measurement maps, 80 weighted generation runs, and 80 weighted refinements | `evidence/*-native-reference.json` |

REA 6.5.0 and Ghidra 12.1.4 produced the native evidence. Ghidra was downloaded from its official release, verified against published SHA-256 `ddac49f903da9d5bac833e5cc79395098b9c33cfd3279be5f31bd00387d2d4db`, and extracted inside this workspace. Existing JDK 21 supplied Java. Global Codex/MCP configuration was not changed. See [REA's Windows provider documentation](https://github.com/morluto/rea/blob/main/docs/windows-ghidra-p0.md).

## Architecture and engine boundaries

The executable imports QtCore4, QtGui4, QtNetwork4, MSVCR71, MSVCP71, Windows system libraries, and `lapack_win32.dll`. The numerical import includes `dgesv_`. ImageMagick 6.4.2 and dxflib 2.2.0.0 source/version strings also appear in the binary. These observations identify components and leads; they do not establish which codec or numerical path executes for every input.

The main application at `0x424650` creates a `QApplication`, registers resources, reads language settings, constructs the GUI, and enters `QApplication::exec()`. The inspected startup routine does not expose a headless conversion branch. The PE export directory is empty. This does not prove that every possible private entry mechanism is absent.

The engine has separate stage entry points. Qt thread constructors establish their names and vtables; their run routines call the engine methods below.

| Stage | Qt run routine | Engine entry | Important downstream routine |
| --- | --- | --- | --- |
| Palette finding | `0x436af0` | `0x46db00` | `0x48f2a0` |
| Segmentation | `0x436b60` | `0x46df50` | `0x488d20` |
| Contour smoothing | `0x436bc0` | `0x46e0e0` | `0x497c30` |
| Bezier fitting | `0x436c20` | `0x46e200` | `0x4a1030` |

These boundaries support a future shared engine API with separately callable stages and retained intermediate state. They do not establish an existing stable ABI that can be exported as an independent product.

Representative REA evidence IDs:

- Overview: `ev_8adfd9e7fed0dd1a669d24563b6e295579ba5aa46806910ded9092607d5ef581`.
- Settings dossier: `ev_73c1db978b1d6afde02689cb7e07088dafb7874c38f3120bb1f5bfc94e3dce5d`.
- Segmentation dispatch: `ev_b9c5a644394adf372b8ef84cbaefe5a3f649e22f992f1890d08dba717a7f508f`.
- Three-phase smoothing: `ev_a115ae50d7cb6adff1b8ca91d8c7efabbea3b8d74a60a82799da510e4edca0b3`.
- Bezier dispatch: `ev_d9ac7990f3e5661f1d8f03d37fc25884ddece0441f9613591df32094c7fb2f0a`.

## Settings and numerical behavior

`0x46b9a0` translates user settings into core settings. `0x46e9d0` loads a configuration through a string stream; `0x4a1260` parses named parameters into registered integer, float, and double destinations. The ten embedded profiles are therefore referenced by live processing code, rather than merely appearing in unused text.

`0x4401b0` identifies category values: `0` photograph, `1` artwork with blending, `2` artwork without blending, and `3` auto-detect. `0x4403a0` identifies quality values: `0` high, `1` medium, `2` low, and `3` auto. For resolved basic categories, the translator selects from the pointer table at `0xa21d08` using `3 * category + quality`. The nine entries refer to profile indices `9, 8, 7, 6, 5, 4, 3, 2, 1`. The auto-detection decision rules remain unresolved.

The advanced-mode translator includes these mathematical mappings. Variable names are interpretations, anchored by persistence keys and field offsets. The expressions below simplify the x87 instructions mathematically. The implementation uses recorded host-CPU values for all 12 slider positions, preserving the observed float32/float64 storage rather than replacing it with approximate JavaScript interpolation.

| Setting | Recovered expression |
| --- | --- |
| Segmentation complexity `c` | `lambda_pre = lambda_initial = 0.0001 * (13 - c)` |
| Segmentation complexity `c` | `lambda_final = 0.05 * 400^((12 - c) / 11)` |
| Bezier complexity `b` | Initial threshold `0.0001 * (13 - b)` |
| Bezier complexity `b` | Final threshold `0.1 * 200^((12 - b) / 11)` |
| Contour smoothness `s` | Each phase interpolates geometrically: `low * (high / low)^((s - 1) / 11)` |

The smoothing endpoint tables are preserved in `evidence/settings-tables.json`. Anti-aliasing enabled uses prior arrays `[0.05, 0.01, 0.45]` to `[10, 10, 10]` and length arrays `[0.25, 0.05, 0.005]` to `[10, 1, 1]`. Disabled uses prior arrays `[0.05, 0.01, 0.45]` to `[20, 20, 20]` and length arrays `[0.5, 0.5, 0.5]` to `[40, 40, 40]`. Slider construction call sites establish integer ranges 1 through 12. Original defaults remain separate questions; the new resolver requires explicit controls.

The settings probe runs the original arithmetic spans with relocated absolute constant operands and the original 53-bit x87 precision setting. It records 72 segmentation and 576 smoothing/fitting combinations. An earlier Unicorn experiment disagreed with the host in 888 recorded fields, mainly transcendental rounding. Those emulator results remain diagnostic evidence and do not supply production constants or test expectations. See `evidence/settings-emulation-comparison.json`.

The profile extractor initially counted 97 parameters because its row separator pattern missed three single-dot optimizer entries. The corrected extractor and registry agree on all 100 parameters in every profile. The clustering switch at offset `0x218` is separate from this registry and is written only by advanced segmentation.

The smoothing dispatcher explicitly iterates over three phases. The profiles also expose prior types, measurement types, corner puncturing, length penalties, air pressure, and conjugate-gradient parameters. The profiles alone do not define their objective functions.

In the recovered Bezier keeper routine at `0x49f310`, adjacent spans merge when the cost increase is below a threshold. The cost helper at `0x49f250` computes `cost(combined span) - cost(left span) - cost(right span)`. Otherwise the keeper routine evaluates moving the shared partition by one sample. The fitting cost at `0x49e960` has distinct paths for different sample counts and calls further numerical helpers. This is partial algorithm recovery, not a complete fitter.

The observed opacity formatter at `0x4760f0` returns `1.00` for alpha bytes at least 251; lower values use two decimal places after division by 255. Its saved query filename, `009-svg-writer.c`, reflects the original search hypothesis; the recovered function is the opacity formatter, not the SVG writer.

## Independently implemented operations

`src/settings.mjs` resolves the basic profiles and advanced numeric controls. The library and `settings` CLI command share this implementation. Its 648 differential cases cover the recorded arithmetic overrides; they do not cover classification, palette transfer, or complete pipeline execution.

`src/palette.mjs` reproduces nearest-palette assignment at `0x48d840`. The original uses SSE float32 normalization, subtraction, multiplication, and left-to-right addition over BGRA, with the first palette entry winning ties. `0x476090` confirms BGRA order by formatting bytes 2, 1, 0 as RGB. Flattening at `0x46c700` treats the source channels as premultiplied. The new assignment passes 1,193 native cases containing 12,032 pixel decisions.

`CoreSession` implements unweighted fixed-count palette generation and refinement. Its random generator starts at 1 and advances by multiplication with 2003 modulo 9973, retaining state across operations. Assembly at `0x468bc0` establishes the x87 floating-point result; Ghidra inferred an integer return type for this routine, so that decompiler signature is not used as the contract. Unweighted refinement at `0x48dc00` passes 89 native calls, including convergence, the 10-pass limit, empty clusters, and state carry-over.

Histogram peak detection at `0x48edb0` uses 8 or 16 bins per BGRA channel, a minimum weight of 2, and either eight axis neighbors or all 80 surrounding bins in four dimensions. Equal neighboring weights remain peaks. Sorting at `0x48ea40` uses a bottom-up linked-list merge schedule. For equal weights, `0x48cbb0` reports true if any left channel is smaller than its right counterpart, rather than a lexicographic comparison. The independent detector and merge schedule pass 44 native scans, including fully tied dense tables.

Unweighted fixed-count generation passes 74 native runs through original histogram construction, peak sorting, initialization, refinement, and final byte quantization. The bounded native harness omits scoring and retained-result projection after inspecting their writes. Finalization at `0x48f320` truncates `channel * 255 + 0.5` under the original x87 precision, stores the byte, and normalizes it back to float32. It does not reassign pixels after quantization. Automatic color-count selection remains unfinished.

Anti-alias measurement at `0x483b40` passes 471 native cases. It reflects top, bottom, left, and right samples inward at boundaries, normalizes BGRA bytes and differences to float32, then accumulates squared differences in double precision. The byte result is the truncated, clamped value of `sqrt(sum) * 255 / sqrt(12)`. This is a contrast map, not an image-type classification. The standalone kernel requires both dimensions to be at least 2; application handling of single-row and single-column images remains unresolved.

The original weight-map constructor at `0x47fc80` initializes strength 50 and minimum 0.01. Weighted histogram and means use `max(1 - measurement * 50 / 255, 0.01)`, preserving the native multiplication order and separate double/float32 storage. Membership collection at `0x47e0f0` instead accumulates raw `measurement / 255` in double precision. Weighted generation passes 80 complete native color-stage runs, and weighted refinement passes 80 additional native runs before final quantization. Both use the same session PRNG and canonical refinement loop as the unweighted branch.

The exact-address engine helper now resolves all 1,814 original imports, initializes six parameter registries, and constructs the original core. It reserves the original address range in its PE image before Windows creates the process heap; original sections can then occupy their expected addresses without relocating instructions. It patches no original code and does not enter application startup. Complete palette stages, including scoring, anti-alias blending, actual vector allocation, and retained-candidate copying, reproduce the bounded captures for ordered prefixes of 69 unweighted and 63 weighted runs. The helper's constructor-backed retained-candidate range limits these particular captures to counts at most 24. This is a reference-harness bound, not an established GUI limit.

`scoreUnweightedPalette` reproduces the candidate scorer at `0x48d180` for 1,235 calls to the unmodified engine. It sums supplied assignments' float32 BGRA distances with float32 accumulation, applies `log10(1 + sum)` and a float32 `(colorCount - 2) * costPerColor` penalty, then adds the rounded results. Half the general cases deliberately use non-nearest assignments. Empty images, 256-color palettes, byte-normalization boundaries, and 4,096-pixel accumulation are included. Scores belong to unquantized refined colors in the original generation pipeline. Weighted scoring is implemented separately by the local blend model described below.

`src/blending.mjs` implements the shared local color-blend solver. All 1,080 native calls match exactly for one through nine colors, including float32 fitted colors, double weights, and double self weights. Two colors use BGRA projection and the original regularization. Multi-color fitting uses BGR, regularization 0.01, a sum-to-one constraint, and iterative zero constraints for weights below -0.0001. It preserves smaller negative residuals and the sample alpha. Assembly shows an additional precision detail: multi-color blue and green byte normalization retain double products, while red and alpha pass through float32 stores. This difference explains the first numerical mismatch; preserving it removes all observed weight and color drift. Native factor and constraint snapshots remain in the recording for further differential investigation.

`scoreWeightedPalette` matches 440 original scorer calls with explicit assignments, palettes up to 256 colors, empty maps, narrow maps, and larger random boundaries. It reproduces the original radius-2 boundary mask, eight-neighbor identifier order, blend fit, float32 squared errors and accumulation, and color penalty. Existing palette code and the blend model share one prepared-pixel/palette validation boundary in `src/pixel-input.mjs`. Automatic anti-alias segmentation now integrates presegmentation and palette transfer; integrated prepared shadow dispatch now has separate complete native comparisons. Its dispatcher first runs the unweighted initializer and then the separate subpixel stage; a nearest-palette assignment is a different branch controlled by the core's palette mode. These paths are not substituted for each other. See `docs/blending-contract.json` and the REA bundle through request 275 in `evidence/rea-blending-evidence-bundle.json`.

`src/subpixel.mjs` rebuilds subpixel region statistics and energies. All 540 native snapshots match, covering 19,076 region records and 77,160 cached pixel costs across eight- and 24-neighbor processing. The comparison includes dense connected labels, float32 counts and moments, channelwise upper medians or means, sorted boundary sets, and double interior/total errors. Out-of-bounds neighbors do not make a pixel a boundary. Interior-error arithmetic clamps BGR variance cancellation while preserving alpha residuals. Total region error includes cached costs for outside pixels in its boundary set. The original initializer requires completed base-region statistics; the native harness establishes that precondition with the original base rebuild. No original instructions are patched. See `docs/subpixel-contract.json` and the REA bundle through request 290 in `evidence/rea-subpixel-evidence-bundle.json`.

`createSubpixelModel` adds persistent, owned state with directed merge costs and explicit applied merges. All 10,708 native proposal evaluations match, including 38 early lower-bound rejections. Assembly corrects the decompiler at `0x48af40`: each recomputed pixel cost rounds to float32 before double accumulation. All 838 incremental merge states across 340 cases also match, including subsequent proposals after cache changes. Native merges keep the target color, keep stable identifiers with emptied source slots, preserve the source's all-pixel squared sums, and update interior moments only for common boundary pixels that become interior. Newly interior cache values remain populated. Candidate selection and the complete unclustered schedule are implemented separately below. Original mutation traces through request 295 are retained in `evidence/pseudocode/`.

`model.iterate` now reproduces candidate selection at `0x4a8640`. All 2,700 ordered native passes match complete numerical snapshots and return counts, including 18,350 accepted merges, 95,380 region records, and 385,800 pixel-cache entries. Sources follow identifier order; candidates follow sorted boundary-pixel order. Source limits select all pixels or interior pixels, while target limits always use total pixels. Equal costs keep the first candidate. Pair deduplication preserves the original uint32 packing `(source << 16) | target`; a per-pass set matches the original end-of-pass flag reset at `0x4a8280`. Candidate keys above 65535 preserve assembly-defined collisions, but this corpus does not exercise that large-identifier range. The complete unclustered schedule, rebuild boundaries, and ineffective thin cleanup are verified separately below. The reference bulk process now has a bounded 120-second timeout. Its earlier 30-second process terminated after 2,536 complete passes; that partial file did not supply expectations. The complete native recording repeats exactly.

`model.rebuild` matches all 1,080 native snapshots after explicit merges and alternating eight/24-neighbor processing. It retains the existing pixel cache and overwrites current boundary entries, while rebuilding dense connected labels, moments, medians/means, and boundary sets. The recording includes 36,590 region records and 154,320 cache entries. Original thin cleanup at `0x4a7f20` changes none of 540 valid states, containing 17,775 active zero-interior roots. Assembly skips nonzero-interior roots and rejects every zero-interior root when its boundary contains a foreign pixel. Such a foreign pixel necessarily exists for a valid zero-interior region. The independent schedule therefore preserves the original ineffective behavior without inventing a pixel optimizer.

`optimizeSubpixel` matches all 432 original final states from `0x4a9ac0`, including 216 optional-wide cases. It follows the original constructor limit of 20 repeats after each phase's first pass, all source/target limit changes, rebuilds, optional wide-neighborhood processing, and final diagonal repair. The shared raw-pixel diagonal implementation moved into `src/region-data.mjs`; existing unweighted behavior still matches 994 repair maps and 328 complete segmentations. These 432 comparisons start from explicit prepared initial regions. They exclude preceding unweighted presegmentation, subsequent shadow/color clustering and palette transfer, and all later vectorization stages. The evidence bundle through request 302 is `evidence/rea-subpixel-schedule-evidence-bundle.json`.

`presegmentRegions` matches 632 original 0x4ab730(0) states across all ten profiles and selected advanced settings. It runs axis passes with the actual minimum-region setting even when the later diagonal-extraction flag is enabled. The canonical `segment` API and CLI now select the automatic anti-alias branch as well as the existing unweighted branches. All 240 complete anti-alias runs match labels, counts, quantized palette floats and color identifiers, including 168 basic and 72 advanced unclustered cases. The branch uses true presegmentation, the complete subpixel schedule, first-encounter byte-color deduplication, palette-label connected rebuilding, and final minimum-region cleanup. A fresh core defaults clustering off; explicit advanced clustering true invokes the recovered color-selection controller. Supplied-palette segmentation is described below; nonzero size penalties remain unfinished.

The known-settings pipeline oracle now captures the palette immediately after segmentation and generates a minimal differential fixture with its current recording digest. Independent segmentation matches all 15 stages, including prepared filtered pixels, labels, counts, palette floats, and color identifiers. Later smoothing, fitting, geometry and exporters still use original instructions only in the reference harness. Matching these segmentation stages does not establish independent complete vector conversion.

`model.selectColors` implements original `0x4a83a0`, and the advanced `clusterColors` switch now reaches it through both the library and CLI. All 548 selected-color states match, including 48 actual color replacements and eight stress cases with more than 100 eligible candidates. The original orders candidates by descending interior count and descending identifier, then keeps only 100. It changes small-region model colors and identifiers while retaining labels, moments, boundary sets, and cached pixel costs. Earlier copied colors remain visible to later fits. Its bounded boundary routine at `0x48b090` accumulates unrounded double pixel energies, unlike the float32-per-pixel path used for merge costs. All 7,832 raw bounded evaluations match exactly. The shared kernel now returns raw energy and each consumer applies its own native rounding rule.

Clustering-enabled automatic anti-alias segmentation matches 240 complete original runs; eight outputs differ from the equivalent unclustered cases. The CLI reproduces native advanced results at complexities 1, 6 and 12. That corpus explicitly disabled shadowing; it does not describe the native fresh constructor defaults. Integrated shadow comparisons are covered below. Supplied-palette behavior is described below. The native selection and full clustered recordings repeat exactly. The evidence bundle through request 308 is `evidence/rea-color-selection-evidence-bundle.json`.

Input update at `0x4738b0` reduces images above 200,000 pixels toward a 150,000-pixel working image using two resampling branches. Their exact resampler mapping remains unrecovered. `0x472c30` explicitly imports a BGRA byte map. These findings establish additional loading and large-image acceptance cases; the current library still consumes already prepared engine-format pixels.

`src/pixel-map.mjs` owns row-major signed int32 segment identifiers. Its independent breadth-first flood fill produces the same pixel mutations as the original scanline routine at `0x484100` for 5,574 native cases. These include every binary topology up to 3 by 3 at every seed, narrow maps, extreme identifiers, and larger combs. The reference harness redirects deque calls to its own bounded container and relocates seven direction operands; the original integer scanline instructions perform the reference writes. The new implementation has no native harness dependency.

The `fill`, `assign-palette`, `refine-palette`, and `generate-palette` CLI commands expose these operations as JSON. Palette generation/refinement requires an explicit `antiAlias` boolean; the weighted branch additionally requires dimensions. Missing mode fields fail rather than selecting an old input shape. Component contracts describe these boundaries under `docs/`. The npm file allowlist contains only authored source, recovered numeric facts, README, package metadata, and the MIT license. Original instruction bytes, binaries, assets, and investigation evidence stay outside the package.

`src/segmentation.mjs` implements recursive tile initialization, float32 color merge costs, connected-region statistics, union-root merging, boundary pixel moves, iteration scheduling, minimum-size cleanup, and palette projection for unweighted segmentation. Separate native recordings cover 1,042 color costs, 300 initial maps, 945 single passes, and 994 diagonal-contact repairs. Complete segmentation matches 160 original runs without diagonal extraction and 168 runs with it, covering all seven unweighted profiles. Inputs range from single pixels through 32 by 32 images and include constant, striped, random, low-contrast, alpha, checkerboard, and diagonal patterns. The comparisons include labels, counts, palette floats, and region color identifiers. The independent stage also matches labels, counts, color identifiers, and output colors in all ten unweighted known-settings pipeline captures.

The diagonal branch temporarily disables minimum-region enforcement, adds an eight-neighbor pass, repairs diagonal contacts, rebuilds connected regions, and restores the original minimum size. Repair uses local second-difference color costs, outside-facing corner checks, and asymmetric float32/double candidate comparisons. It changes labels without updating statistics until the next rebuild. Diagonal palette extraction uses float32 byte-space means with truncation, while ordinary projection rounds normalized means. The original small-region color search starts at the negated constructor value of 10. Its squared error cannot be negative, so it retains each region's own color. The independent implementation preserves that result without reproducing the ineffective search or sort.

The `segment` CLI supports all nine basic category/quality combinations using explicit controls and prepared BGRA bytes. Automatic anti-alias processing includes presegmentation and palette transfer. Advanced anti-alias clustering is supported; nonzero size penalties fail explicitly while unfinished. These are bounded component results, not whole-image output parity. The segmentation contract is in `docs/segmentation-contract.json`; original-code traces through request 234 are retained in `evidence/rea-diagonal-evidence-bundle.json`.

## Feature contract and reference corpus

`docs/reconstruction-contract.json` records the required features, evidence, recovery status, and acceptance criteria. The surface includes automatic/basic/advanced settings, palette editing, transparency, segmentation, smoothing, curve fitting, manual segmentation tools, undo/redo, background removal, batch conversion, six vector exporters, bitmap rendering, saved configurations, and partial recomputation.

The embedded overview help lists SVG/EPS/PDF. Later release notes add AI/EMF/DXF, and the shipped outputs demonstrate all six. The reference corpus uses those six formats; runtime behavior still needs testing with known settings.

| Shipped input | Dimensions | SVG paths | Cubic commands |
| --- | --- | --- | --- |
| Logo With Blending Small | 250 x 250 | 8 | 92 |
| Logo With Blending | 400 x 400 | 13 | 326 |
| Logo With Transparency | 250 x 250 | 6 | 223 |
| Logo Without Blending | 298 x 268 | 8 | 244 |

These exports establish concrete geometry and serialization examples. Their generation settings are not supplied, so they cannot independently verify the settings translator or automatic classification. New known-settings cases are needed for every mode and boundary condition.

## Known-settings native pipeline references

The exact-address helper now runs original BGRA import, input filtering, fixed-count palette generation, segmentation, contour extraction, all smoothing phases, Bezier fitting, output assembly, and six-format export. Fifteen ordered images cover split regions, diagonal boundaries, nested rings, alpha transitions, and three-way junctions under profiles 3, 6, and 9. All 100 typed parameters are recorded per image. This closes the earlier lack of known-settings engine references; GUI interactions, file decoding, automatic modes, and independent full conversion remain unverified.

The harness initializes the original ImageMagick input cache and clears the per-image filter-applied flag exactly as the image-transfer routine does. Omitting the cache caused a native ErrorCache exception on anti-alias segmentation. Omitting flag invalidation would skip filtering on later images. With both included, the photograph alpha case changes 232 prepared byte channels through the original noise filter. The capture records those working pixels instead of assuming loading preserves bytes.

Geometry output follows the live region edges to used cubics. Original fitting can reserve unused slots containing uninitialized memory, so allocated storage is not a list of valid curves. The original edge renderer at 0x4741b0 confirms that kind 0 uses consecutive contour nodes, kind 1 draws a cubic forward, and kind 2 reverses it. Captures preserve region parents, node positions, indices, and all referenced cubic coordinates without serializing pointers.

Per-stage snapshots show that anti-alias smoothing consumes the palette PRNG. The routine at 0x495a30 takes two uniform random values per perturbed point. The independent full engine must share this state across stages and images; a palette-only session cannot account for subsequent smoothing calls by itself.

The original extension dispatcher at 0x47cfb0 writes SVG, PDF, EPS, AI, DXF, and EMF. Ninety known-settings files are stored in evidence/engine-reference-exports. Repeat runs reproduce all 15 numeric geometries and 75 export files byte-for-byte. The 15 EPS files differ only in their creation-date line. That clock-dependent field remains part of the output contract. SVG XML is parsed, while the other file checks establish signatures and selected structural markers rather than full semantic validity. These results validate the reference harness, not independent whole-image parity. See docs/engine-reference-contract.json.

## Supplied-palette segmentation

`segment` and its CLI accept normalized BGRA palette colors. Plain mode reuses verified nearest assignment and repairs diagonal topology before connected rebuilding and final minimum-size cleanup. The initial native connected rebuild and repeated assignment reduce to that same repaired color map. Anti-aliased mode runs true presegmentation and the complete subpixel schedule, always selects colors, then projects each unassigned model color onto the supplied palette. Palette floats, ordering, duplicates and unused colors survive transfer.

The fixed projection at `0x4a7c80` uses float32 channel subtraction followed by double products and accumulation in BGRA order. Assembly contradicts the reversed float32 decompiler expression; captured control word `0x027f` confirms 53-bit x87 precision. The 1,644-state corpus includes threshold enforcement, repeated projection, twelve close winners and twelve duplicate ties below a double ulp. All state fields match. Complete supplied-palette segmentation matches 782 original results: 302 plain and 480 anti-aliased.

The unrestricted original plain branch faulted on a 1x1 image with three palette entries. Its first connected rebuild allocates component statistics, then `0x489300` writes one record per palette color without ensuring sufficient capacity. A retained native preflight identifies 482 such corpus inputs and records them under `unsupportedCases`. It does not patch or replace original instructions. These inputs are excluded from final-output parity assertions. Independent connected rebuilding safely handles them; that behavior has no successful original final result to compare. The failure evidence and scope remain explicit.

## Independently recovered contours

`src/contours.mjs` implements original 0x499eb0 contour extraction before smoothing. The native node order visits the left border, right border, top interior border, bottom interior border, then interior boundary grid points row-major. The four image corners and multi-region junctions retain kind 3. Region tracing follows left, down, right, up candidates with the original inside-pixel tie rule and closing-edge metadata. It builds sorted adjacent-region lists, removes ordinary straight border nodes, applies six constructor patterns to long plain-image staircases, compacts shared node identifiers and preserves incoming sample counts. Nesting parents follow the original asymmetric adjacency rule.

The implementation matches all 2,584 native states. These include 1,156 retained segmentation results, 1,312 exhaustive binary grid cases, 112 reflected long staircases and four nested rings. The oracle imports completed region maps and verifies each segmentation field before tracing. It records initialized boundary flags rather than three allocation-padding bytes. Fresh native recordings repeat byte-for-byte. Independent segmentation and contour extraction also match all 15 raw contour stages captured inside complete original pipelines. The npm export is `ambivalence/contours`; the CLI command is `trace-contours` with prepared pixels and processing settings. One-dimensional preprocessing, smoothing, fitting and export remain unfinished.

## Smoothing numerical components and complete-stage targets

`src/smoothing.mjs` implements the default type-3 curvature prior at 0x494730/0x494840 and the quadratic fallback at 0x493ee0/0x494580. Every recovered GUI profile selects type 3, but the total objective overrides it for regions smaller than eight pixels and also adds quadratic flag-0x10 inversion penalties. Both kernels preserve original sample counts, double operation order, owned derivative vectors and public input validation. The curvature kernel matches 5,010 finite native energies and endpoint derivatives; six collapsed-edge cases retain native NaNs. The quadratic kernel matches all 5,016 cases. Independent finite differences confirm endpoint derivative semantics. The first decompiler-based derivative expression disagreed in low bits; assembly established the correct product order and every retained comparison now uses exact equality.

The native pipeline now captures geometry immediately after original 0x497c30 and before curve fitting. Its 15 target states move 409 nodes and alter 192 node kinds/flags, retaining the shared PRNG state before and after each image. The decoded preparation applies blend-driven shifts before uniform anti-alias jitter; replacing that preparation with Gaussian noise or plain jitter would change behavior. The prepared geometric objective, border constraints, quadratic line search and Fletcher-Reeves controller are now independently implemented. Prepared image-based measurement and both profile line-search modes are also independently implemented. Blend-driven preadjustment, shared-PRNG jitter, other optimizer modes, corner puncturing and complete multi-phase dispatch remain unfinished. Fixed nearby-edge construction is independently implemented. Prepared anti-alias coverage repair is now independently implemented. These numerical components do not yet produce independent complete smoothed coordinates. The authoritative recovery map is `docs/smoothing-contract.json`.

`smoothingObjective` matches all 5,598 native prepared energies and constrained gradients exactly. It preserves float32 reference-cache coordinates, sample counts, region-order accumulation, small-region signed-area pressure, reference dead zones and byte-count-scaled inversion measurements. The node-flag-0x10 quadratic coefficient is `anti_inv_pot_prior_scale`, divided by phase strength before final scaling; it is not `length_barrier_weight`. Independent finite differences check free coordinate derivatives.

`smoothingLineSearch` matches 3,312 original quadratic searches, including accepted coordinates, energy, returned delta, next epsilon, accumulated step and line count. It preserves two sequential trial moves, bounded parabola displacement, tie handling, backup restoration, the fixed 0.5 epsilon decay and the phase step-derived epsilon fraction. Original 0x493640 resets image borders and clamps the fixed cache of nearby interior nodes after every move. The cache is an explicit prepared input rather than a guess based on moved coordinates.

`smoothingOptimize` matches all 588 complete native Fletcher-Reeves controllers with optimizer and line-search type 0. The corpus spans all phases and varied stopping/restart controls, with native iteration fields reaching 260. Final coordinates, energy, stored gradient, direction, best-energy baseline, beta, epsilon, step, iteration field, restart count, line count and knockout countdown all match exactly. Stopping can occur before refreshing the stored gradient or incrementing the iteration field, so those fields are reported with their original meanings. These are prepared geometric runs with image-based measurement and anti-alias coverage refresh excluded; they do not establish complete original smoothing parity. Ghidra has no function definition at 0x493610; retained exact assembly confirms its restart side effect is inactive for these geometric models.

The prepared controller also implements original line-search type 5 at 0x4ad3e0, used by the recovered anti-aliased artwork profiles. All 441 native seeing/blind runs match every captured coordinate and controller field exactly. The controller alternates quadratic seeing moves and rolling-step blind estimates, caps and scales blind displacement by 0.8, restores rejected coordinates without the post-move callback, and immediately retries a seeing search. The corpus proves rejected estimates in 187 runs and cycle wraps in 321 runs. Native blind-only iterations retain the preceding accumulated-step field. This completes both line-search modes used by the recovered profiles at the prepared geometric boundary. The image-based integration below extends that boundary; coverage refresh and full smoothing remain unfinished.



## Stateful image-based contour measurement

`src/image-measurement.mjs` and its private kernel independently implement the image-only measurement at 0x488710/0x487fe0 from prepared pixels, completed segmentation and fixed shared contours. The model initializes float32 cached colors from region labels, traces double contour/grid intersections, stores float32 coordinates and perimeter values independently, inserts pixel corners, preserves native short-run removal, integrates signed region areas and nesting parents, and applies original invalid-coverage handling. Channel normalization and residual subtraction use float32; energy accumulation, interpolation Jacobians and raw node gradients use original double order. Assembly corrected misleading decompiler casts.

Every field matches 2,802 original initial states and 6,981 stateful evaluations across 1,425 coordinate sequences, spanning all three phases. Comparisons include energies, pixel colors, byte coverage flags, every raw gradient channel and 3,272,031 initialized clipped records. The corpus includes all 15 original post-smoothing pipeline geometries in each phase, exact quarter-pixel error, and near-horizontal directions at exponents -70/-90. An independent geometric check confirms the quarter-pixel error and free interior derivatives. Exactly 652 original sequences retain different error after restoring their initial coordinates; resetting the pixel-color cache on every call would change this behavior.

`src/arctangent.mjs` models the observed extended angle-register boundary in independent JavaScript arithmetic. Original FPATAN retains its extended result through subtraction while separately storing a rounded double angle for the next edge. Subtracting two already-rounded Math.atan2 results left six clipped-record cases different. Preserving that register/subtraction sequence resolves those differences without clamping tiny angles or omitting initialized fields. Relative guard precision also covers the retained near-horizontal reference cases.

The public export is `ambivalence/image-measurement`. `createContourImageModel` owns its prepared pixels, region colors, contour topology and persistent cache. Its `evaluate(positions)` method accepts explicit flat double coordinates and returns owned energy/gradient/color/flag/record snapshots. The same owned model now supplies prepared smoothing objectives and both profile controller modes. Prepared contour-repair callbacks are now implemented. Independent nearby-edge preparation and shared PRNG initialization are now implemented. Corner puncturing and full phase dispatch remain unfinished. The authoritative contract is `docs/image-measurement-contract.json`.

## Prepared image-based smoothing integration

Prepared smoothing now accepts the matching owned image model for measurement code 1. Exact native comparisons cover 2,799 combined energies and constrained gradients, 3,312 explicit quadratic line searches, 1,029 complete Fletcher-Reeves controllers using line types 0 and 5, and 6,981 combined objective states across 1,425 persistent sequences. Energy evaluation rebuilds records and updates cached colors once. Gradient-only refresh reuses those records and adds each pixel derivative directly into the accumulated prior gradient. Computing a separate summed image gradient and adding it afterward changes rounding.

Expanded phase coverage exposed a folded-contour error in perimeter selection. Native turn fields are float32, but subtracting them and adding 2*pi uses double precision. Retaining that precision resolves sourceObjective 2217 and its downstream line-search trajectory without weakening equality assertions. The case remains a durable regression.

The integration simplification review ran inline in one context without independent corroboration. It applied no additional reuse changes, one quality change replacing positional evaluation booleans with named modes, and one efficiency change skipping unused geometric energy accumulation during derivative-only refresh. It skipped scratch-buffer pooling and angle memoization. All 104 tests pass in 174 seconds after these changes. All six image-related native recordings repeat byte-for-byte. Strict x86 Clang, Python syntax, all 11 contract JSONs, three pipeline-fixture provenance checks and all 43 installation-file hashes pass. No lint or typecheck script is configured. The original pipeline still repeats 15 numeric cases and 75 exports exactly; 15 EPS files differ only in creation dates. The approved compiler smoke test remains absent, and no new unreachable code was found.

## Prepared anti-alias contour repair

`src/smoothing-repair.mjs` independently implements signed segment crossing, normalized adjacent-joint crossing, probe direction/winding seeds, per-region repair, global penalty decay and sorted invalid-region repair. The private image kernel now records regions whose full turn or pixel coverage is invalid. Exact comparisons cover 14,106 retained states across 1,425 moved/repeated/restored sequences, including all 2,850 native image-error selections and their 274 nonempty sets. The native preparation interval is 10 in every case. Repair changes flags and byte penalty counts without moving coordinates. Shared node updates retain the original region order, and penalties decay to one rather than zero.

`smoothingOptimize` accepts explicit fixed `nearbyEdges` and `repairInterval` preparation state for anti-alias callbacks. All 1,029 complete original repair-enabled controllers match coordinates, energy, stored gradients, directions, stopping/restart fields and final node flags/penalties. Both profile line-search modes and all phases are covered, with native iteration fields reaching 445. Repairs change coordinates in 150 runs, final energy in 155 and restart counts in 152. Changes restart from the stored gradient, reevaluate energy and reset the best baseline before stopping checks; they do not reset the seeing/blind cycle. All retained repair and controller regressions now construct original preparation associations independently and compare them with recorded native lists before use. Preadjustment/jitter, puncturing and full phase dispatch remain unfinished.

The repair simplification review ran inline in one context without independent corroboration. It applied one reuse change sharing binary node-state decoding, one quality change attaching the controller documentation to the correct function, and one efficiency change preserving native short-circuiting before a segment-length square root. It skipped direction memoization and scratch pooling. All 107 tests pass in 208 seconds after these changes. Both new native recordings repeat byte-for-byte. Strict x86 Clang, Python syntax, all 11 contract JSONs, three pipeline-fixture provenance checks and all 43 installation-file hashes pass. No lint or typecheck script is configured. The original pipeline still repeats all 15 numeric cases and 75 exports exactly; 15 EPS files differ only in creation dates. The approved compiler smoke test remains absent, and no new unreachable code was found.

## Independent contour neighborhoods

`prepareContourNeighborhoods({ geometry })` is exported through `ambivalence/smoothing`. It independently constructs the original two-dimensional sliding-midpoint tree over ordered contour samples, including repeated shared nodes, and returns owned `nearbyEdges` and byte `flags`. It preserves both partition passes, axis/cut ties, native near/far traversal, epsilon 1e-12 pruning, stable capped distance insertion, the min(50,nodeCount) request, strict squared-distance <25 filtering and exclusion of samples starting at the query node. Existing node mask 0x01 skips preparation; processed nodes receive that mask. The original maximum-distance constant is verified from source bytes as Number.MAX_VALUE.

All 2,814 explicit native preparations match exactly: 64,518 nodes and 1,045,003 ordered associations. Twelve added variants exercise collapsed points, duplicate/tied coordinates, both sides of the distance threshold, large/tiny anisotropy, reflection and pre-marked nodes. The independent contour extractor and neighborhood builder also reproduce all 2,454 original preparation lists used by the retained repair/controller corpora, totaling 958,636 associations. All 14,106 repair states and 1,029 complete repair-enabled controllers now use those independently built lists.

The neighborhood simplification review ran inline in one context without independent corroboration. Shared coordinate-range and partition helpers already serve tree construction, with no extra reuse change. It applied one quality change removing an unused loop binding and one efficiency change retaining the selected axis range instead of scanning it again. It skipped priority-buffer pooling and cross-node query memoization. All 109 tests pass in 191 seconds afterward. Strict x86 Clang, Python syntax, all 11 contract JSONs, three pipeline-fixture provenance hashes and all 43 original installation-file hashes pass. The neighborhood recording repeats byte-for-byte; both retained repair recordings also remain byte-identical. The original pipeline still repeats all 15 numeric cases and 75 exports exactly, with creation-date-only differences in 15 EPS files. No lint or typecheck script is configured, no new unreachable code was found, and the approved compiler smoke test remains absent.

## Remaining recovery work

1. Recover automatic color-count selection, classifier rules, loading/resampling, one-dimensional inputs and separate shadow-classifier paths.
2. Extend unsupported numerical modes and native failure domains beyond the verified basic-profile smoothing and default-flag fitter contracts.
3. Recover shared document structures, edits, background flags, stage invalidation and cancellation behavior.
4. Complete document/export integration and environment acquisition. All six vector serializers are implemented for their documented fitted-graph contracts.
5. Complete image-input conversion, the integrated CLI and custom GUI, then compare the complete application against the reference before publication.

Ghidra's automatic function inventory is incomplete for some relevant addresses. For example, assembly at `0x476170` contains the SVG header-writing routine, but it was not returned as a defined procedure and the Windows REA read-only route cannot create that function definition. A nearest preceding function is not a valid substitute. The full objdump disassembly is retained for these gaps.

The original application launched successfully. The user confirmed that it opens normally and Save/Export works. Computer Use screen capture failed with `FrameArrived timed out` and `window capture timed out`; text-only inspection returned mostly unnamed Qt panels. No new reference export was obtained through GUI automation. The private native core harness now supplies the known-settings pipeline references described above. The original was left open for inspection. Startup may write its own Qt preferences; the installation files were not intentionally changed.

## Reproduction and verification

Run the static extractor from the workspace:

```powershell
python .\tools\extract-static-evidence.py 'D:\VectorMagic\Vector Magic\vmde.exe' .\evidence
```

It verifies the executable digest before applying the recovered resource-table addresses. Resource paths, file offsets, payload hashes, profiles, strings, and installation hashes are recorded. The canonical resource paths are in `evidence/qt-resource-index.json`; the old extra `evidence/qt-resources/Resources/Resources/` directory is a duplicate from the first extraction and is excluded from that index.

`tools/rea-session.mjs` connects to REA through its published MCP launcher and processes the saved request JSON files in order. Set `REA_PACKAGE_ROOT`, `GHIDRA_INSTALL_DIR`, and `JAVA_HOME` to the actual tool paths. It uses one live Ghidra session for follow-up queries. The numbered evidence files preserve complete responses and their limitations. The exported bundle and snapshot retain results; they are not an editable Ghidra project.

The static extractor completed with ten profiles and 386 resources. Recovered resources and the installation manifest were checked against their SHA-256 values. The 99 Node tests cover the component contracts and CLI parsing/serialization without mocks. Native reference probes reproduce their recordings as documented in `docs/reference-probes.md`. No lint or typecheck script is configured for this JavaScript package. Inline simplification retains one refinement loop, groups the matched measurement/weight arrays, and removes per-pixel neighbor-array allocation while preserving arithmetic order. The scorer reuses boundary validation and the verified float32 distance operation; the inline reuse, quality, and efficiency pass found no further worthwhile source changes. This was one review context, without independent reviewer coverage. All native comparisons still pass. Full image-output parity has not been tested.

The native helper simplification pass separated input preparation, geometry capture, and export ownership into coherent functions and reused the wide-string initializer. The inline review applied one reuse change and three quality changes; it added no efficiency changes and skipped one low-impact proposal to reorder binary geometry serialization. No lint or typecheck script is configured. Native recordings and the existing Node suite verify the changed harness boundaries; no independent review contexts were used.

The segmentation simplification pass retains one owned optimizer state and public validation boundaries. The diagonal update reused the existing optimizer, float32 scale, palette deduplication, and connected rebuild. Its inline reuse, quality, and efficiency review applied one quality change and two efficiency changes: removed a single-item test loop, per-line channel arrays, and a parent array immediately replaced by rebuilding. It skipped broader fixture-pattern consolidation and splitting the coherent diagonal scan into more helpers. No validation or arithmetic ordering was removed. This was one review context without independent corroboration. All 45 tests pass; no lint or typecheck script is configured. The native helper passes strict x86 Clang syntax checks. Offline installed exports match 328 segmentation cases, 994 diagonal repairs, and all three artwork CLI quality levels.

The blending simplification review ran inline in one context, without independent reviewer coverage. It applied one reuse change and three efficiency changes: canonical shared input validation, cached solves for unchanged constraints, allocation-free boundary-mask expansion, and reused per-pixel scoring buffers. It made no quality-only changes and skipped broader matrix abstractions and fixture-generator consolidation. Exact solver weights, all score fields, and the full 45-test suite remain green. No lint or typecheck script is configured. Strict x86 Clang syntax checks and Python compilation pass. The offline installed package reproduces all 1,080 blend calls, 440 weighted scores, and 1,235 unweighted scores and runs the existing palette CLI.

The subpixel simplification review ran inline in one context without independent reviewer coverage. It applied two reuse changes, one quality change, and one efficiency change: shared blend kernels and neighborhood ordering, shared connected-region rebuilding, direct statistics ownership, and removal of transient image/snapshot copies. It skipped broader matrix abstractions and median histogram optimization. All 47 tests pass. No lint or typecheck script is configured. The original subpixel reference contains 540 rebuilds and repeats exactly. The offline installed package matches those rebuilds, 1,080 blend calls, 440 weighted scores, 1,235 unweighted scores, and 328 segmentation cases. Native pipeline repeatability still matches all 15 geometries and 75 exports byte-for-byte, with only creation dates differing in the 15 EPS files. All 43 installation files remain unchanged. Full anti-alias optimization and complete conversion remain unfinished.

The subpixel merge simplification review ran inline in one context without independent reviewer coverage. It applied two reuse changes and two efficiency changes: shared error refresh, shared differential-state assertions, removal of retained caller labels from persistent state, and region-owner tracking in the reference corpus generator. It made no quality-only changes and skipped lazy error refresh and broader neighborhood abstraction. All 51 tests pass. Strict x86 Clang checks and Python compilation pass, and the three subpixel native recordings repeat exactly. The offline installed package matches all 10,708 proposal costs, 838 applied merge states and subsequent costs, and the previous blend, scoring, and segmentation corpora. No lint or typecheck script is configured.

The merge-pass simplification review ran inline in one context without independent reviewer coverage. It applied one reuse change and two efficiency changes: shared per-region error refresh, trusted internal cost/mutation calls after validating pass controls once, and error projection at proposal/snapshot reads instead of whole-state rescans after every internal merge. It made no quality-only changes and skipped matrix-factor memoization and broader optimizer abstraction. The initial straightforward implementation matched all 2,700 pass states before optimization, and all 53 tests remain green afterward. The pass comparison takes about 110 seconds on this host; no speedup claim follows from runs with different concurrent native workloads. No lint or typecheck script is configured. Strict x86 Clang, Python compilation, and contract JSON checks pass. The native pipeline still repeats all 15 geometries and 75 exports exactly, with creation dates accounting for all 15 EPS differences. All 43 original installation files remain unchanged. The offline installed package matches 240 ordered pass states across 48 selected cases, plus the complete prior subpixel, merge, blend, scoring, and unweighted segmentation corpora. Every packaged file matches current source, and the 14-file archive excludes all original artifacts.

The subpixel schedule simplification review ran inline in one context without independent reviewer coverage. Shared topology and neighborhood helpers already avoided duplication. It applied one quality change to the extracted scan's indentation, with no further reuse or efficiency changes. It skipped broader schedule-loop abstraction and matrix-factor memoization. All 58 tests pass, including both the existing 2,700-pass corpus and the new 432 complete schedules. Strict x86 Clang checks, Python compilation, and contract JSON checks pass; no lint or typecheck script is configured. Both new native recordings repeat exactly. The native pipeline still repeats all 15 geometries and 75 exports byte-for-byte, with creation dates accounting for all 15 EPS differences. The offline installed package matches all 1,080 rebuild states, 42 selected complete subpixel schedules, and the previous merge, blend, scoring, segmentation, and 994 diagonal-repair cases. Every packaged file matches current source, and the 14-file archive excludes original artifacts. All 43 installation files remain unchanged.

The integrated-segmentation simplification review ran inline in one context without independent reviewer coverage. It applied two reuse changes, one quality change and one efficiency change: shared core-parameter serialization, shared final-segmentation capture, corrected palette-generator indentation, and projected-label snapshots that avoid copying unused moments between stages. It skipped broader optimizer abstraction and merging distinct native palette quantization rules. All 62 tests pass; a separate CLI check covers all nine basic combinations. Strict x86 Clang, Python compilation and contract checks pass; no lint or typecheck script is configured. Both new recordings and the unweighted recordings repeat exactly. The pipeline repeats all 15 numeric results and 75 export files byte-for-byte, with only creation dates differing in 15 EPS files. The offline installed package matches all 632 presegmentation, 240 anti-alias, 328 unweighted and 15 pipeline-segmentation cases, and all nine CLI combinations. The 14-file archive matches current source and excludes original artifacts. All 43 original installation files remain unchanged.

The color-selection simplification review ran inline in one context without independent reviewer coverage. Shared blend fitting, energy kernels, state snapshots and parameter validation already avoided duplication. It applied one quality change documenting the raw-energy/cache invariants and one efficiency change collecting eligible candidates directly into one array before sorting and truncating it. It applied no further reuse changes and skipped broader ownership refactoring and matrix-factor memoization. All 67 tests pass, including the prior merge and schedule corpora after the precision change. A new CLI validation assertion initially expected different wording; it now checks the exact unchanged settings-validator message. Strict x86 Clang, Python compilation and contract JSON checks pass; no lint or typecheck script is configured. The offline installed package matches all 548 selected-color states, 7,832 raw boundary costs, 240 clustered and 240 unclustered anti-alias runs, 328 unweighted runs, 15 pipeline segmentation stages, nine basic CLI combinations and three advanced clustered CLI cases. All 43 original installation files remain unchanged.

The supplied-palette simplification review ran inline in one context without independent reviewer coverage. It reused nearest assignment, diagonal repair, connected rebuilding and shared final cleanup. The review applied no extra reuse changes, one quality comment documenting the recorded x87 precision, and no efficiency changes. It skipped broader boundary/copy refactoring and native fixture-generator consolidation. All 73 tests pass in 188 seconds. Strict x86 Clang, Python compilation and contract JSON checks pass; no lint or typecheck script is configured. Both new numeric recordings repeat byte-for-byte. The offline installed archive matches 782 supplied-palette results, 1,644 projection states, 548 selected-color states, 7,832 raw boundary costs, 480 automatic anti-alias results, 328 unweighted results, 15 pipeline stages, nine basic CLI combinations, three advanced clustered cases and two supplied-palette CLI cases. Its 14 files match source and contain no original artifacts or runtime dependencies. The 482 unsafe native precondition inputs remain outside parity assertions. All 43 original installation files remain unchanged. The unused compiler smoke test remains removed as requested. REA evidence through request 310 is in `evidence/rea-supplied-palette-evidence-bundle.json` with 192 records.

The contour simplification review ran inline in one context without independent reviewer coverage. It applied one reuse change by sharing the initialized-contour parser between the stage oracle and full pipeline capture, one quality change excluding three uninitialized boundary padding bytes, and one efficiency change removing per-node temporary label arrays. It skipped broader tracer-context restructuring and scratch-buffer pooling. All 80 tests pass in 171 seconds. Strict x86 Clang, Python compilation, contract JSON validation and both pipeline fixture provenance checks pass; no lint or typecheck script is configured. The 2,584 native contour states repeat byte-for-byte. The pipeline repeats all 15 numeric results and 75 exports exactly; 15 EPS exports differ only in creation dates. All 43 installation files remain unchanged, and the unused compiler smoke test remains absent. The comparison preview renders identical original and independent contour panels. REA evidence through request 315 is in `evidence/rea-contour-evidence-bundle.json` with 196 records.

The previous contour-neighborhood checkpoint verified a 21-file offline-installed package. That historical archive is superseded by the Ambivalence contour-initialization checkpoint below.

The smoothing-prior simplification review ran inline in one context without independent reviewer coverage. Shared point/count validation already serves both kernels; no extra reuse refactor was needed. It applied one quality change giving shared validation neutral smoothing-prior errors and made no efficiency changes. It skipped a transient vector wrapper and premature scratch-buffer pooling. All 85 tests pass in 187 seconds, including exact prior comparisons, independent finite differences, collapsed-edge behavior and existing component/CLI regressions. Strict x86 Clang, Python compilation, all contract JSONs and three pipeline fixture provenance checks pass; no lint or typecheck script is configured. The 5,016-case prior recording repeats byte-for-byte. The original complete pipeline still repeats its 15 numeric cases and 75 exports exactly; 15 EPS files differ only in creation dates. All 43 installation files remain unchanged and the unused compiler smoke test remains absent. Complete independent smoothing remains unfinished.

REA evidence through request 401 is in `evidence/rea-contour-initialization-evidence-bundle.json` with 269 records. Crossing, winding, penalty-decay and restart callbacks and fixed nearby-edge construction are independently implemented at the prepared boundary. The native numerical functions and target smoothing snapshots remain development-only; neither the installed application nor native analysis tools are runtime dependencies.

The smoothing-controller simplification review ran inline in one context without independent reviewer coverage. It applied no additional reuse or quality changes and three efficiency changes: replaced a one-use area buffer with a scalar, skipped unused derivatives during line-search energy evaluations, and avoided temporary axis arrays in the clamp callback. Native arithmetic and public validation remain unchanged. It skipped premature scratch pooling and broader phase-context restructuring. The initial straightforward implementation matched every objective, line-search and complete-controller field before optimization; those exact comparisons remain green afterward. All 92 tests pass in 169 seconds, with a scoped 12-test check after the final local simplification. Strict x86 Clang, Python syntax, all contract JSONs and three pipeline-fixture provenance checks pass. No lint or typecheck script is configured. All four smoothing recordings repeat byte-for-byte. The original complete pipeline still repeats 15 numeric cases and 75 exports exactly, with creation dates accounting for the 15 EPS differences. All 43 installation files remain unchanged; the approved compiler smoke-test removal remains in effect. No new unreachable code was found.

The follow-up seeing/blind implementation review also ran inline in the same context. Shared quadratic search, objective kernels, move constraints and validation already served both controller modes; no further worthwhile reuse, quality or efficiency change was found. It skipped scratch-buffer pooling and factoring two small displacement-cap expressions whose native negative-product order differs. All 94 tests pass in 174 seconds, and the final scoped 14-test smoothing check passes. All five smoothing recordings repeat byte-for-byte. The offline archive additionally reproduces all 441 seeing/blind controllers. Strict x86 Clang, Python/contract validation, three pipeline provenance checks and all 43 installation-file hashes pass. No lint or typecheck script is configured, no new unreachable code was found, and the approved smoke-test removal remains in effect. Image-based measurement energy, gradient, edge coverage and per-pixel color/gradient routines are retained as the next recovery inputs, with no independent measurement parity claimed yet.

The image-measurement simplification review ran inline in one context without independent reviewer coverage. It applied no extra reuse changes, two quality changes (a simpler axis branch and documentation of native predecessor handling), and three efficiency changes: scalar pixel/region derivative channels, scalar interpolation weights, and allocation-free coordinate validation. Native product/addition order and trust-boundary checks remain intact. It skipped premature angle memoization and broad scratch-buffer pooling. The straightforward implementation matched every initial and sequential native field before optimization, and all comparisons remain green afterward. All 99 tests pass in 176 seconds. Both new native recordings repeat byte-for-byte; the complete original pipeline still repeats 15 numeric cases and 75 exports exactly, with only creation dates differing in 15 EPS files. Strict x86 Clang, Python syntax, all contract JSONs, three pipeline-fixture provenance checks and all 43 installation-file hashes pass. No lint or typecheck script is configured. The approved compiler smoke test remains absent, and no new unreachable code was found.

No public repository, npm release, original activation data, or original executable/DLL distribution was created.

## Ambivalence contour initialization checkpoint

The project, npm package and terminal command are named `ambivalence`. The
independent implementation remains MIT-licensed and free. GitHub publication is
authorized once the requested converter and GUI are complete and verified.
The package remains private while full parity is unfinished.

`preadjustContours` matches all 2,604 native blend-driven preparation calls.
`CoreSession.initializeContours` matches all 2,604 complete original preparation
and jitter calls, with 111,576 raw uniform draws and exact retained states.
The same CoreSession owns palette randomness. All 110 original interleaved
palette-generation and contour-initialization runs match every recorded output.
Border nodes consume no draws; plain images are unchanged; zero-amplitude
anti-alias initialization still consumes two draws per interior node.

The implementation preserves float32 reference-coordinate truncation, repeated
quad palette identifiers, skipped first/self blend entries, +/-0.333 clamping
and native arithmetic order. The diagonal coefficient is 1/Math.sqrt(2), whose
last bit differs from Math.SQRT1_2. Palette operations round random colors when
storing float32 values; contour jitter consumes the raw double uniforms.
All three new native recordings repeat byte-for-byte.

The simplification review ran inline in one context, without independent
corroboration. It applied two quality changes covering precision documentation
and sparse-region validation, plus one efficiency change reusing a four-channel
pixel sample per call. It found no further useful reuse change and skipped
per-pixel blend memoization and moving the canonical session class to another
module. No new unreachable code was identified.

The full 113-test suite passed in 190,590 ms before the final sparse-region guard.
The final 33-test scoped run, including that guard and the Ambivalence CLI rename,
passed in 2,276 ms. Strict x86 Clang, Python syntax and all 11 contract JSON checks
pass. Three pipeline fixtures and three initialization fixtures have verified
recording provenance. All 43 original installation files remain unchanged, and
the approved compiler smoke-test removal remains in effect. Fresh original
processes still reproduce all 15 numeric pipelines and 75 byte-identical exports;
creation dates account for the remaining 15 EPS differences.

Full smoothing still requires reference-grid and fixed-border preparation,
corner puncturing and complete phase dispatch. Fitting, exporters, image decoding,
classification, editing/document behavior and the custom GUI also remain
unfinished. No full image-output parity or GitHub publication is claimed.

The contour-initialization checkpoint archive contains 22 files and nine public entry
points, with zero runtime dependencies and no original artifacts. Every archive
member matches both current source and a fresh offline installation outside the
project at `D:\migracion\ambivalence-verification\contour-initialization`.
The 56,813-byte archive SHA-256 is `85750a2f54e3f4d26cf4e915b203336b8d5c9e68f232601756e4ae6e4913ff78`.
Installed checks reproduce all 2,604 preadjustments, 2,604 initializations,
110 interleaved runs, 323 palette generation/refinement calls, 2,814 neighborhood
preparations, 1,029 repair-enabled controllers, 2,584 contour states, 15 pipeline
segmentation/contour stages and four CLI contour cases. The canonical proof is
`evidence/npm-package-verification.json`; it records the actual installed scope
rather than inheriting older installed checks. npm normalizes CRLF shebangs on
installation, so the Ambivalence launcher now uses LF throughout. Archive/source/
installed byte-equality checks remain strict.

## Raw contour preparation checkpoint

`prepareContourSmoothing` now independently prepares owned raw grid geometry,
float32 reference positions, zero penalty bytes, compacted left/right/top/bottom
border groups, the fixed near-border list and anti-alias neighborhoods. Plain
inputs preserve flags and empty neighborhoods. Anti-aliased inputs mark prepared
flags before initial movement. Every field matches all 2,700 original 0x496740
preparations, including 128,160 node states, 21,042 near-border entries and
2,620,080 neighborhood associations. Preparation consumes no random draws.

`selectNearBorderNodes` also matches all 2,700 explicit original 0x482490 calls.
The 96 direct threshold probes include 48 adjacent-double values and 48 float32
neighbors, both axes and both edges, and every unrolled-loop remainder. The
original executable stores 1.100000023841858 as both float32 and float64 constants;
x87 assembly uses double coordinate comparisons throughout. Decompiler bottom
edge float casts were misleading. Strict comparisons are preserved.

Independent preparation composes exactly with all 2,604 retained-session native
initializations. The new preparation recording repeats byte-for-byte and its
fixture matches the recording bytes. REA evidence through request 404 is retained
in `evidence/rea-contour-preparation-evidence-bundle.json` with 271 records.
A subsequent analysis recovered the small corner decision tree at 0x4adfb0;
its feature/statistics extraction and independent behavior remain pending.

The simplification review ran inline in one context without independent
corroboration. It applied two quality changes: explicit border-group selection
and true neighboring float32 threshold probes. One efficiency change computes
fixed upper-edge thresholds once per call. Existing neighborhood construction
was reused; no further reuse change was needed. It skipped relocating this
coherent preparation module and consolidating boundary validators with different
error contracts. No new unreachable code was found. Bash remains unavailable,
so the skill context bootstrap ran directly with Node.

All 117 tests passed in 208,367 ms after the final source changes. Strict x86
Clang, Python syntax, all 11 contract JSONs and three pipeline fixture provenance
checks pass. All 43 original installation files remain unchanged. The native
pipeline still repeats 15 numeric results and 75 exports exactly; the other
15 EPS files differ only by creation dates. The approved compiler smoke-test
removal remains in effect. No lint or typecheck script is configured.

The raw-preparation checkpoint archive contains 22 exact-source files,
nine public entry points and zero runtime dependencies or original artifacts.
Its SHA-256 is `1f357c687f2f263401a14ee7c7fbe7a0621e61599eda6bcf8a1c27d4b38b1fb4`. A fresh offline installation outside the
project at `D:\migracion\ambivalence-verification\contour-preparation` matches every
archive member byte-for-byte. Installed checks cover all 2,700 preparations and
near-border selections, 2,604 prepared initializations, the prior initialization
and palette corpora, 2,814 neighborhoods, 2,584 contours, 15 segmentation/contour
pipeline stages and four CLI cases. Existing prepared optimizer tests passed in
the full source suite; they were not rerun in this focused installed check.
`evidence/npm-package-verification.json` records this actual scope.

Full smoothing still requires corner feature statistics/classification,
puncturing and phase dispatch. Fitting, exporters, image decoding, classification,
editing/document behavior and the custom GUI remain unfinished. GitHub
publication is authorized after completion; no repository or release was created.

## Complete corner puncturing checkpoint

`punctureContourCorners` independently reproduces original 0x494a90. It returns
owned geometry, preserves coordinates/topology/existing markers, skips contours
of six samples or fewer, and applies native cyclic seven-node windows, normalized
edge directions, local-maximum ties and the fixed corner decision tree. A positive
corner decision marks the shared node kind 3 and boundary bit 0x01. A local turn
maximum marks boundary bit 0x08. No original artifacts are runtime dependencies.

All 3,504 original classifier decisions match, including 1,248 adjacent-double
threshold cases and 576 nonfinite feature vectors. All 13 double thresholds were
read from executable bytes. Native 0x4adfb0 returns AL only, so the harness
zero-extends that byte. Its x87 unordered comparison branches are preserved;
ordinary rewritten positive comparisons would differ for NaN inputs.

All 3,680 complete native puncturing states also match. These include the raw
contour corpus, 1,029 repaired optimizer results, 15 fully smoothed original
pipeline states and 32 transformed/collapsed/premarked cases. They cover 189,170
eligible windows and 29,211 skipped short-region states. Puncturing changes
25,461 node kinds, sets 66,034 boundary corner bits and sets 118,378 turn-maximum
bits without clearing old markers or changing coordinates/topology. The corpus
includes finite coordinates that overflow intermediate differences, exercising
unordered behavior in complete windows as well as the isolated classifier.

Only the six fields read by the fixed original classifier are evaluated. Its
unused length and median feature fields are not exposed as a public feature API.
The straightforward kernel matched all native cases before optimization. An
inline simplification review in one context applied two efficiency changes:
compute each directed turn once per contour and reuse fixed scratch vectors.
No extra reuse or quality change was needed. It skipped merging corner turns
with the regularized prior kernel, whose arithmetic differs, and replacing the
explicit classifier with a generic tree interpreter. There was no independent
reviewer corroboration. No new unreachable code was found. Bash remains
unavailable, so the context bootstrap ran directly through Node.

Both new recordings repeat byte-for-byte and both fixtures match their recording
bytes. REA evidence through request 408 is retained in
`evidence/rea-contour-corners-evidence-bundle.json`, with 274 records.
The saved 407 helper name is a hypothesis: 0x493810 copies length features;
0x4938e0 performs a partial selection statistic. Their unused output fields are
not required by the recovered fixed decision tree.

All 120 source tests passed in 175,562 ms after the final implementation changes.
The final scoped 11-test corner/preparation run also passed. Strict x86 Clang,
Python syntax, all 11 contract JSONs and two new fixture provenance checks pass.
The native pipeline repeats all 15 numeric cases and 75 exports byte-for-byte;
creation dates account for 15 EPS differences. All 45 numeric cases across the
three pipeline stage fixtures stayed unchanged during this fresh capture.
All 43 original installation files remain unchanged. The approved compiler
smoke-test removal remains in effect. No lint or typecheck script is configured.

The last verified corner checkpoint archive contains 23 snapshot-source files,
nine public entry points, zero runtime dependencies and no original artifacts.
Its SHA-256 is `0db26e5202a4afea6159babf199574209f68d69dd397dc3b50915c12ceb0c934`. A fresh offline installation outside the
project at `D:\migracion\ambivalence-verification\contour-corners` matches all archive
members byte-for-byte. Installed checks cover all 3,504 classifier decisions,
3,680 complete puncturing states, 2,700 preparations/near-border selections,
2,604 prepared initializations, the existing initialization and palette corpora,
2,814 neighborhoods, 2,584 contours, 15 raw pipeline stages and four CLI cases.
Prepared optimizer checks passed in the full source suite rather than being
rerun in this focused installed check. The canonical installed scope is recorded
in `evidence/npm-package-verification.json`.

At that historical corner checkpoint, full smoothing remained unfinished. Fitting, exporters, decoding,
automatic classification, editing/document behavior and the custom GUI also
remain unfinished. GitHub publication remains authorized after completion and
verification; no repository or release was created.

## Complete supported-profile smoothing

The internal `smoothContourGeometry` joins raw preparation, one persistent
image model, session-owned initialization, enabled phases in native order,
optimizer repair state, per-phase puncturing and final all-region repair/decay.
`CoreSession.smoothContours` exposes this operation with the retained application
random stream and owned outputs. A thrown library call preserves caller input
and restores its stream. No public reset, seed override or second stream is
exposed. The prepared `smooth-contours` CLI resolves explicit settings, emits
geometry/penalties as JSON and reports failures without partial stdout output.
It does not perform image decoding, segmentation, fitting or export.

All 15 established original full-smoothing targets match exact geometry, final
penalty bytes and recorded PRNG states. The broader corpus now retains all
2,713 outcomes across two explicit original process sessions: 2,712 completed
vectors and one fatal outcome. Coverage includes all nine basic profiles, all
eight phase masks, three corner schedules and both supported line-search modes.
All outcomes repeat identically, including session boundaries. Native failed
context contains addresses, so whole failed binaries are not byte-repeat proofs.

The first harness fault, at case 297, required the original diagnostic registry
constructor 0x484fd0 for registry 0xa69048. Exact startup bytes at 0x6f8e00 show
that initialization. No original instruction or diagnostic branch was patched.
A distinct original phase-2 fault at case 775/0x486b6d reproduces with stage-start
PRNG state 4789; the same input at state one succeeds. The capture records eleven
contiguous fragment groups at pixel (2,2). IDs at model+0x78 and double weights
at model+0xa0 overlap on overflow, overwriting the count at model+0xf0 with
1315640065. Geometry, clipped records and scratch bytes are retained in
`evidence/full-smoothing-fault-state-775.json`.

One packed numeric record now preserves those aliased fields, original operation
order, stale bytes and one-past-count background subtraction. Successful native
overflow cases still match; the known invalid access raises a JavaScript
exception. This compares conversion outcomes without claiming identical native
process crashes or library recovery semantics. The explicit previously failing
conformance test passes. No fragment cap, skipped case, tolerance change or
image/seed special case was introduced. After retaining the fatal outcome, the
reference recorder starts a fresh original process for all 1,937 following inputs.
Shared wire definitions were moved into `tools/engine-reference-codec.py` and
are used by both existing probes and the complete-session recorder.

The inline ce-simplify-code review used one context without independent reviewer
corroboration. It applied two quality changes (redundant Python imports and an
unused test binding) and two efficiency changes (reusing penalty storage across
phases and caching the count during read-only scratch search). No extra reuse
change was needed; two broader ownership/module changes were skipped. No new
unreachable code was identified. Bash was unavailable, so the context bootstrap
ran directly through Node. No lint/typecheck script is configured.

All 127 source tests pass in 581,906 ms without failures or skips. The fresh
offline installed copy also passes every complete smoothing outcome and all
15 pipeline targets, plus the retained palette, initialization, neighborhood,
corner, preparation, contour and CLI checks. Its 24 files match archive, source
and installed bytes exactly, with nine public entry points and zero runtime
dependencies. The 62,043-byte archive SHA-256 is
`30b99367155f231139ee79526d755d2e9188e00f5c1370380e939c0be85c6a6c`.
Proofs are `evidence/installed-full-smoothing-verification.json` and
`evidence/full-smoothing-package-archive-verification.json`; the canonical
`evidence/npm-package-verification.json` now records this snapshot. Strict
x86 Clang, Python syntax and all eleven contract JSONs pass. All 43 original
installation files remain unchanged, and the approved compiler smoke-test
removal remains in effect. REA evidence is retained through request 415 in
`evidence/rea-smoothing-full-evidence-bundle.json` with 278 records.

Other prior/optimizer modes and nonfinite controller trajectories remain pending.
Fitting, exporters, image decoding/preprocessing, automatic classification/count
selection, editing/document behavior and the custom GUI remain unfinished.
The full goal stays active. GitHub publication remains authorized after completion
and verification; no repository or release was created.

## Fitting sample preparation reference

REA queries 416 through 428 trace the original fitter's sample preparation,
quadratic/cubic fit paths, normal-equation solve, keeper selection and refinement.
Original 0x49c7b0 accumulates interior chord lengths with sqrt(dy*dy + dx*dx),
includes the final endpoint in total length and multiplies by its reciprocal.
Collapsed total length uses one as the divisor. Wrapped intervals repeatedly add
the contour length to end indices below start. Optional marking sets each visited
shared interior node to kind 2. Adjacent/empty spans return zero without marking.

Native helper operation 44 isolates that function using explicit shared nodes,
a cyclic contour, start/end indices and the marking switch. It uses the existing
fitter at core+0x1f08 and captures only initialized numeric sample fields, final
node kinds and retained random state. Original vector allocations are released
through the matching original delete[] entry. No image, keeper decision or fit
solver is invoked by this probe.

The new recorder retains 2,784 original sample-preparation states and 7,408
interior samples from all 15 smoothed pipeline geometries, eleven complete-stage
geometries, explicit repeated/collapsed/shared-node contours, wrapped/full-cycle
spans and scale probes down to 2^-90. No case consumes random state. At that initial checkpoint this was a reference capture; the independent
fitting implementation and its full comparisons are described below.
`docs/fitting-contract.json` records its boundary before implementation. The
native binary and fixture repeat byte-for-byte. An inline simplification review
of the settled probe/recorder found no needed changes or unreachable code; it
used one context without independent corroboration. REA evidence through request
428 is retained in `evidence/rea-fitting-samples-evidence-bundle.json`, with
291 records. Final strict x86 Clang, four Python syntax checks, all twelve
contract JSONs and the original installation integrity checks pass.

## Independent default-flag shared Bezier fitting

`fitContours`, exported from `ambivalence/fitting`, now owns and fits completed
smoothed geometry. `fit-contours` accepts that graph and explicit settings as
JSON. It validates once at the public boundary, preserves caller storage, emits
only complete geometry and reports failures on stderr with no partial stdout.
The fitter consumes no application random draws.

Sample preparation matches all 2,784 original states and 7,408 interior samples.
All 2,784 interval costs and control-point sets match across the empty, one/two
sample, quadratic and cubic branches. The independent normal equations retain
the original 0.00001 diagonal regularizer, zero-seeded dot-product order,
unblocked LU scaling/pivot/update/triangular arithmetic, residual evaluation and
quadratic power-coefficient conversion. Short branches preserve the original
quarter controls and first-sample-only straight error rather than substituting
standard interpolation control points.

Keeper selection matches all 3,907 native states, including active physical
slots, starts/spans, links, cached costs, dirty bits and final thresholds. The
corpus includes 84 iteration/fraction schedules and 1,039 isolated multiplier
probes. Original x87 thresholds differ from Math.pow on several profiles.
Shared extended arithmetic now preserves FYL2X/F2XM1, native register constants
and the intervening 53-bit rounding boundaries. Binary helpers moved from the
angle module without changing its algorithm; broad image/smoothing regression
checks pass against that shared implementation.

The complete controller normalizes markers, selects shared keeper intervals,
reserves native cubic slots including unused sentinels, fits each shared run
once, and emits region line/cubic edges in original order and direction. Every
field matches all 2,712 original completed smoothing states across all nine
basic profiles, plus all 15 retained full-pipeline targets. One upstream fatal
smoothing outcome has no geometry to fit and remains retained as that failure;
no fitter input is omitted. Uninitialized reserved coordinates are excluded
while their total slot count stays observable.

Native modes 44/45/46/47 isolate sampling, interval costs, keeper partitioning
and complete fitting. The shared fitted-geometry codec also drives the original
pipeline parser. All four numeric recordings repeat byte-for-byte. Native
reference evidence remains development-only. The original extra refinement
flag stays zero in this implemented/captured stage; its alternate branch remains
explicit pending recovery.

The inline ce-simplify-code review used one context with no independent reviewer
corroboration. It applied one reuse change (one proposal evaluator), one quality
change (separate captured RNG metadata from pure results), and two efficiency
changes (reuse proposal scratch and retain coefficient tables once). Three
broader changes were skipped to preserve ownership, arithmetic and module
boundaries. No new unreachable code was found. Bash remains unavailable; the
context bootstrap ran directly through Node. No lint/typecheck script exists.
All 136 tests in the completed full suite pass in 580,023 ms. A subsequent
public-pipeline integration test also passes, bringing current test definitions
to 137; the final 25-test fitting/CLI/pipeline run passes in 3,595 ms. No runtime
source changed after the full suite. A fresh original pipeline capture preserves
all 75 numeric cases across five stage fixtures. Its 75 non-EPS exports repeat
byte-for-byte; creation dates account for the 15 EPS differences.

The 68,214-byte archive contains 27 source/package files and ten public entry
points, with zero runtime dependencies. SHA-256 is
`8d7dabf41261b4a9fd8f964cf6159233c3c5977fe1afe3e43e91fc09aa9b552a`.
A fresh offline installation at `D:\migracion\ambivalence-verification\fitting`
matches every archive/source/installed byte and reruns all fitting corpora,
complete smoothing and retained component checks. It also verifies three fitting
CLI categories and the library ownership/validation boundary. Package proofs
are `evidence/installed-fitting-verification.json` and
`evidence/fitting-package-archive-verification.json`; the canonical package
proof now points to this snapshot. Vector document/output assembly,
serializers/options, image decoding/preprocessing, automatic classification/count
selection, editing/document behavior and GUI remain unfinished. The original
full goal remains active; publication is still reserved for completed parity.


The public numerical APIs also match all 15 original prepared pipelines in one
retained CoreSession: palette outputs, region segmentation, raw shared contours,
full smoothing/penalties/random states and complete fitted geometry. Neither the
source test nor the installed check uses recorded seed overrides or private stage
entry points. `test/pipeline.test.mjs` retains this coupled comparison; its fixture
is generated by the original pipeline driver. Installed proof is
`evidence/installed-prepared-pipeline-verification.json`. This does not establish
file decoding, automatic classification/count selection or serialized exports.

## Independent output assembly and SVG

`src/vector-output.mjs` reproduces child lists, hole-edge joining, color grouping
and hidden-bit visibility. All 2,872 original states match: 2,712 fitted inputs
and 160 visibility probes, with 1,095 child links, 994 hole paths, 2,360 hole-edge
pairs and 112,496 visibility queries. The native last-match junction rule is
preserved by a Map after the naive quadratic scan passed the same corpus.

The confirmed SVG dispatcher is 0x47ca40, constructor 0x4759e0 and vtable
0x8dcfa0. Earlier filenames labeling 0x47cd80/0x476370 as SVG were hypotheses;
the dispatcher proves those are PDF. The contract records this correction.
Unrecognized tiny methods were inspected with exact objdump bytes, without
inventing function boundaries or repeatedly requesting unavailable procedures.

`exportSvg`, from `ambivalence/svg`, validates the fitted graph and options once.
The CLI `export-svg` accepts identical JSON from a file or stdin and writes raw
SVG bytes. It preserves layout/group order, hole reversal, hidden-region rules,
premultiplied BGRA bytes, alpha cutoff, opacity, optional thin stroke, dimensions,
CRLF and path spacing. Invalid input produces stderr/exit 1 with no partial SVG.
Every byte matches 17,436 original files over 2,743 geometries. The public prepared
pipeline also matches all 15 original default SVG exports in one retained session,
without private stage calls or recorded seed overrides.

Extended finite-coordinate probes exposed two distinct formatting differences:
MSVCP71 repeatedly scales large magnitudes by 1e-10, and MSVCR71 performs a
17-digit intermediate decimal conversion. The first approximation passed most
inputs but failed two raw cases and was replaced. The independent formatter
reproduces truncated 16-bit product diagonals, 80-bit nearest-even rounding,
radix-eight power composition and 88-bit fractional digit extraction. Effective
power constants are derived from exact rational powers; the retained table
observations verify that derivation. All 16,406 original raw formatting strings
match. No expected strings or input-specific correction table enter the runtime.

Original assembly, raw formatting, SVG and full-pipeline recordings repeat
byte-for-byte. All 43 installation files remain unchanged. The source fixture
contains the original SVG bytes and per-file hashes so the integration test
needs no local evidence directory. The original analysis target was restored
after inspecting its immutable MSVCP71/MSVCR71 runtimes. The formatting evidence
bundle retains 324 records in `evidence/rea-msvcr71-formatting-evidence-bundle.json`.

The ce-simplify-code pass ran inline in one context; it provides no independent
review corroboration. Bash was unavailable, so its bootstrap ran through Node.
It applied zero reuse changes, three quality changes (clear decimal power-step
naming, direct zero insertion and appending long hole runs without spread arguments) and one efficiency change (an export-local
coordinate cache). Two broader factoring/number-format substitution ideas were
skipped to preserve the boundary and exact arithmetic. No unreachable elements
were found. No lint/typecheck scripts are configured. All 27 scoped tests pass
in 8,634 ms; the full suite passes all 146 tests in 576,176 ms.

The current 73,688-byte offline archive contains 30 files and 11 public entry
points, with zero runtime dependencies. SHA-256 is
`8d1a62b794e81242a5519723bfeff32ebbc94880d3d659ed859bf466d9a1e777`.
A fresh install at `D:\migracion\ambivalence-verification\svg-final` verifies every
archive/source/installed byte, all new native comparisons, six SVG CLI cases,
error/help behavior and 15 public pipelines through SVG. All pre-existing numeric
runtime modules are byte-identical to the previous verified fitting archive.
Proofs are `evidence/installed-svg-verification.json`,
`evidence/svg-package-archive-verification.json` and the canonical
`evidence/npm-package-verification.json`. Reproduce with
the verifier from the installation directory with absolute script/archive paths.

Other serializers, file decoding/preprocessing, automatic classification/count
selection, document edits and the custom GUI remain unfinished. The full goal
stays active. No GitHub repository, commit or npm publication was created;
publication remains authorized for completion of the requested application.

EPS recovery is read-only so far. The dispatcher confirms wrapper 0x47c970,
constructor 0x478540 and vtable 0x8ddb44. Its color acceptance at 0x477380 uses
alpha >50, rather than SVG's >10. Coordinate transform 0x478f30 computes
(outputWidth*x)/sourceWidth and ((sourceHeight-y)*outputHeight)/sourceHeight
in that order. Shape/group endings use float32 RGB normalization and precision
three. Missing small function definitions were handled by retained exact bytes.
The complete SVG and initial EPS evidence bundle contains 332 records at
`evidence/rea-svg-complete-eps-recovery-evidence-bundle.json`. Controlled EPS
captures and an independent serializer remain pending.

## Independent EPS and shared vector traversal

`exportEps`, from `ambivalence/eps`, and the `export-eps` CLI now reproduce all
41,010 original EPS files over 3,005 fitted geometries. The input graph and
common options are validated once through `readVectorOutput`. Both SVG and EPS
use `walkVectorOutput` for region order, group boundaries, outer/hole paths and
line/cubic direction. Their format-specific color/coordinate/stream behavior
stays in each writer. Every prior SVG comparison remains exact.

EPS preserves full/compact PostScript, three-decimal float32 RGB, two-decimal
paths, thin stroke, output dimensions, hidden regions and alpha >50 acceptance.
An early mismatch on an empty hidden color group recovered an additional native
state transition: RGB starts in general precision three and changes to fixed
formatting only after the first drawn coordinate. All 256 byte values are now
captured in hidden groups, rather than special-casing the first observed zero.

Coordinate transforms use 53-bit register arithmetic with extended exponent
range before double storage. A BigInt dyadic implementation retains native
subtraction/multiplication/division order. This matches finite extreme inputs
that would overflow too early with ordinary JavaScript, as well as 72 captured
final-store overflow files. Legacy stream infinity text is retained exactly.
The shared formatter now accepts an explicit precision, with one canonical
17-digit conversion and field-rounding implementation. All 16,406 prior raw
strings and 28,956 new precision-three strings match the original runtime.

CreationDate defaults to the current local clock. Optional single-line printable
ASCII metadata supports reproducible exports and exact native comparisons.
Source tests check the real clock without mocks. Original EPS repeats differ
only in that explicit field: the entire binary response matches after masking
exactly 41,010 date payloads. SVG, both decimal corpora and original pipeline
numeric recordings repeat byte-for-byte. The pipeline fixture also embeds all
15 original EPS files, hashes and actual creation dates; public integration
compares palette through SVG/EPS in one retained session without seed overrides.

The inline ce-simplify-code pass used one context without independent review
corroboration. Bash remains unavailable; its bootstrap ran through Node. It
applied zero reuse changes, one quality change (derive group painting from the
common walker's group guarantee) and one efficiency change (emit cubic operands
without a temporary mapped array). Two broader numeric-helper/general-interface
changes were skipped to preserve scope and arithmetic ownership. No unreachable
elements remain. There are no configured lint/typecheck scripts. All 34 scoped
tests pass in 18,481 ms; all 153 tests in the final full suite pass in 538,619 ms, and the fresh offline
package checks also pass.

The shared capture driver remains in `tools/record-svg.py`, with EPS case
construction in `tools/record-eps.py`. EPS fixtures use deterministic gzip
storage; small SVG/EPS CLI fixtures avoid loading the full corpora for transport
checks. Development probes and fixtures stay outside the npm runtime. The full
application objective remains active; publication still waits for complete
parity, including the remaining formats, image input/classification, document
edits and GUI.

The fresh offline EPS archive contains 31 files and 12 public entry points,
with zero runtime dependencies. It is 76,909 bytes, SHA-256
`d40d618be1011653c6c320009fdf352e8dbffa6d114eee6ae09f1296c64dd97d`. Installation at
`D:\migracion\ambivalence-verification\eps` verifies every archive/source/
installed byte, all SVG/EPS/decimal/assembly corpora, both export CLIs and all
15 retained-session public pipelines through both formats. The pre-existing
numeric modules are byte-identical to the earlier fitting snapshot. Proofs are
`evidence/installed-eps-verification.json`,
`evidence/eps-package-archive-verification.json` and the canonical package proof.
The EPS recovery bundle retains 333 records at
`evidence/rea-eps-complete-evidence-bundle.json`.

PDF recovery is now read-only in `docs/vector-output-contract.json`. Its
confirmed wrapper/constructor/vtable are 0x47cd80, 0x476370 and 0x8dd18c.
The retained format-dispatch evidence corrects old SVG-labelled filenames.
New queries inspect move emission (0x476e60) and object/offset registration
(0x476ae0). Their evidence filenames are hypotheses, not function contracts.
Controlled PDF captures are still required for alpha resources, dimensions,
byte offsets/lengths, grouping, strokes and binary header bytes before writing
an independent serializer.

A verification-working-directory audit found that the earlier SVG verifier
resolved every module from the fresh installation but ran library checks with
the source project as process.cwd(). Its historical proof metadata is corrected
and superseded. The current verifier asserts that process.cwd() equals the
installation directory and reruns both complete SVG and EPS corpora from there.
CLI checks already used the installation directory. This correction changes no
runtime source, native comparison or package bytes.

## Independent PDF and shared scaled formatting

`exportPdf`, from `ambivalence/pdf`, and the `export-pdf` CLI reproduce all
22,014 original PDF files over 3,264 fitted geometries. The API returns owned
Uint8Array bytes. The CLI writes those bytes directly, preserving the four-byte
binary comment rather than converting it to UTF-8. Layout, stroke, dimensions,
hidden-region validation and shared-edge traversal use the same canonical
contracts as SVG/EPS.

The PDF writer preserves native object order, content length, physical byte
positions, xref/trailer bytes, producer metadata and opacity-state tracking.
Initial opacity is 255. State changes emit /Vmt<alpha> and mark only used bytes;
resource declarations retain increasing alpha order. The corpus includes all
accepted alpha bytes 11..255, explicit transitions back to opaque, every byte
color, visibility states, layouts, strokes, dimensions and finite extremes.
Separate structural checks resolve every selected xref entry against physical
bytes and confirm the one-LF stream delimiter after the declared content length.
Controlled captures confirm that MediaBox retains source dimensions even when
output paths scale. This behavior remains exact to the reference.

EPS and PDF now share createScaledOutputFormatter in src/vector-output.mjs.
Its 53-bit register arithmetic, extended exponent range, per-axis coordinate
caches and general/fixed RGB state are unchanged from the verified EPS logic.
All 41,010 EPS files and 17,436 SVG files still match. Public integration compares
all 15 original prepared pipelines through SVG, EPS and PDF in one retained
CoreSession without seed overrides. The fixture embeds the original bytes and
hashes; no test runtime needs the executable or evidence directory.

PDF, SVG and original pipeline numeric recordings repeat byte-for-byte. EPS
repeats differ only in the explicitly recorded CreationDate payloads. Proof is
`evidence/pdf-native-repeatability.json`. Mode 53 uses the original dispatcher
and shares the same closed-file capture as SVG/EPS. PDF fixtures use Latin-1
JSON byte representation and deterministic gzip storage. No original instruction
is patched, and no metadata or header byte is omitted from PDF comparisons.

The inline ce-simplify-code pass used one context without independent review
corroboration. Bash was unavailable; the bootstrap ran through Node. It applied
one quality rename (beginObject describes object declaration/offset registration)
and no reuse or efficiency changes. Three broader factoring/encoding/dispatch
changes were skipped to preserve scope and byte behavior. No unreachable code
was found. There are no configured lint/typecheck scripts. All 40 scoped tests
pass in 19,219 ms; all 159 source tests pass in 540,329 ms without skips.

The PDF checkpoint archive contains 32 files and 13 public entry points,
with zero runtime dependencies. It is 78,990 bytes, SHA-256
`64f1ae585837a8038d44d754c1b3d586e2062392cf443dfb7c138331416567f3`.
A fresh offline installation at `D:\migracion\ambivalence-verification\pdf`
passes all three complete export corpora, decimal/assembly comparisons, export
CLIs and 15 public pipelines. The verifier enforces that installation as its
actual working directory and compares every archive/source/installed byte.
The pre-existing numerical modules remain byte-identical to the fitting archive.
Proofs are `evidence/installed-pdf-verification.json`,
`evidence/pdf-package-archive-verification.json` and the canonical package proof.

The PDF-complete/initial-AI evidence bundle retains 337 records at
`evidence/rea-pdf-complete-ai-recovery-evidence-bundle.json`. The dispatcher
confirms AI wrapper 0x47cbd0, constructor 0x477180 and vtable 0x8dd3c0. Its alpha
acceptance shares the inspected EPS method. AI implementation is described below;
DXF/EMF, image-input/classification, document edits and the GUI remain unfinished.
No GitHub repository, commit or npm publication was created. Publication as
ambivalence is authorized once the requested product is complete.

## AI export

The independent exportAi API and export-ai CLI preserve all 22,014 original
AI files over 3,264 fitted geometries. Mode 54 uses the original dispatcher and
closed-file capture after native fitting/assembly. The corpus includes all
layout/stroke choices, hidden regions, byte colors, scaling and finite extremes;
708 files contain compound paths. Captures keep every ASCII byte and CRLF.

Tiny initialize/shape methods were not recognized by Ghidra. Exact assembly
establishes their header and compound rules. The export controller stores a
has-holes byte at stack offset 0x1c and passes that same byte to both shape
callbacks. A decompiled row-counter expression is not used as the contract.
Hole starts paint the preceding subpath; compounds use u/*u and *U/U, while
color groups use u/U independently. Cubics keep the extra space before C.

AI shares EPS/PDF scaling arithmetic. Its numeric header fields set fixed
formatting before RGB output, so the shared formatter exposes that real numeric
operation. Header bounds/art size keep source dimensions; paths scale to output
dimensions. Empty metadata fields, source creator metadata and the missing
AI3_PaperRect separator are retained for byte parity. The runtime has no original
binary or resource dependency.


AI, SVG, PDF and the original numeric pipeline repeat byte-for-byte in fresh
native runs. EPS differs only in exactly 41,010 recorded CreationDate payloads;
all remaining response bytes match. The public prepared pipeline matches all
15 original AI files in the retained session alongside SVG/EPS/PDF. Proof is
`evidence/ai-native-repeatability.json`.

The inline ce-simplify-code pass used all three rubrics in one context, without
independent review corroboration. Bash was unavailable; bootstrap ran through
Node. Two quality changes clarified the formatter state and flattened native
format selection. No reuse/efficiency change was needed; three broader changes
were skipped. No unreachable code was found. Lint/typecheck are not configured.
All 42 scoped tests pass in 22,466 ms. All 164 source tests pass in 540,098 ms
with no failures or skips. Strict x86 compile/link passes, all 13 contracts parse,
eight probe Python sources parse and all 43 original installation files remain
unchanged. The approved compiler smoke-test deletion remains in effect.

The AI checkpoint archive contains 33 files and 14 public entry points with
zero runtime dependencies. Its 80,553 bytes have SHA-256
`1bd5a348878d8e02996846f339aa2093a73fed114edd6669060bcb8d04fb2d33`.
Fresh offline installation at `D:\migracion\ambivalence-verification\ai`
passes all four complete export corpora, decimal/assembly comparisons, export
CLIs and 15 public pipelines. The verifier enforces that installation as its
actual working directory and compares all archive/source/installed bytes.
Pre-existing numerical modules remain byte-identical to the fitting archive.
Proofs are `evidence/installed-ai-verification.json`,
`evidence/ai-package-archive-verification.json` and the canonical package proof.

Read-only DXF recovery confirms wrapper 0x47cb00, constructor 0x479240 and
nine-slot vtable 0x8ddd40. It shares EPS alpha acceptance and has no-op group/end
callbacks. Its curve writer has a spline branch and two line-reduction branches;
its field writer calls the original CRT with %.16lf and trims fractional zeros.
These observations are not independent DXF parity. Tiny methods missing from
Ghidra were inspected with exact bytes, without repeated unsupported requests.
The latest evidence bundle retains 363 records at
`evidence/rea-ai-complete-dxf-fields-evidence-bundle.json`. Controlled DXF option
and formatting captures remain next. Full application parity and publication
remain unfinished; the active goal stays open.


## DXF coordinate and field kernels

The independent coordinate kernel matches all 126,984 original pairs, including
raw double bits, signed zeros, signed int32 denominator wrapping, negative
divisors, infinities and x87 negative indefinite NaN. The original transform
scales/flips coordinates and divides by int32(source dimension * 72). It uses
the shared 53-bit register arithmetic with extended exponents and final double
storage. All earlier SVG/EPS/PDF/AI byte comparisons remain exact.

Mode 56 captures 28,700 precision-16 CRT strings. Eighteen native special-value
strings pin the infinity, quiet/signaling NaN and negative indefinite labels at
precisions two, three and sixteen. Mode 57 also captures the actual original
trimmed field values from its own file stream, checking group-code spacing and
CRLF. Every represented field matches. The 98 overlong raw fields are retained
without invoking the native 256-byte stack-buffer overflow. There are 109
nonfinite fields. One full original extreme-coordinate export stalled and was
terminated after its 30-second timeout; the helper was confirmed absent after
termination. The independent field formatter reports a DXF diagnostic before
that overflow. It does not reject unrelated hidden geometry.

Mode 55 captures 65,934 complete original DXF files across 3,264 fitted graphs.
Another 144 explicit diagnostic cases carry an actually accepted visible line
endpoint whose isolated original numeric field exceeds the buffer. They are not
successful native file comparisons, and only one full stalled case was timed.
Thirty-six successful files retain the same extreme graphs with all regions
hidden. Every captured fitted graph matches its prior native PDF counterpart.
The full 3,423,079,368-byte native response repeats without a mask, SHA-256
`726ac56886692a4efdc3c2ba74dc3577e4de3d822963f5fcc1ce9cc5ad5954ff`.
SVG/PDF/AI, precision-16, coordinate and numeric pipeline responses also repeat
exactly. EPS differs only in its 41,010 recorded CreationDate payloads.

DXF needs the original float32 BGRA palette in addition to the fitted graph.
Header preparation copies every palette entry. Native source-561 retains 24
palette entries and one distinct region byte color but writes 25 layers,
including the default. That state cannot be recovered from byte colors. Every
DXF geometry reference, CLI case and diagnostic now retains the native palette.
No reconstructed-palette fallback is permitted for the new API. The constructor
at 0x42d7c0 initializes shape mode 2, PostScript style 1, stroke policy 1 and DXF
curve mode 2. Application preference loading remains a distinct recovery task.

References use six indexed gzip NDJSON parts below 64 MiB. Every part checksum,
logical stream checksum, geometry/palette record and preceding case reference
is verified. The original monolithic compressed capture remains local under
evidence; no reference bytes were discarded. GitHub's current ordinary file
limit motivates the partitioning; the source is linked in reference-probes.md.
The read-only evidence bundle retains 365 records at
`evidence/rea-dxf-coordinate-complete-evidence-bundle.json`.

The inline ce-simplify-code pass used all three rubrics in one context without
independent corroboration. Bash was unavailable; bootstrap ran through Node.
One quality rename separates geometry-dimension keys from synthetic dimension
probes. Its coordinate requests remain byte-identical. No reuse/efficiency change
was needed; three broader changes were skipped. No unreachable code was found.
Lint/typecheck are not configured. All 22 scoped tests pass in 19,537 ms. All
167 source tests pass in 542,746 ms with no failures or skips.

The DXF coordinate checkpoint archive contains 33 files and 14 public entry points with
zero runtime dependencies. Its 81,178 bytes have SHA-256
`0c50d67a7583ac57d32089ecb704583f32c3d019375da067541a9a9bc3099ccb`.
Fresh offline installation at `D:\migracion\ambivalence-verification\dxf-coordinates`
passes all coordinate/field/decimal comparisons, all four complete export
corpora, export CLIs and 15 retained public pipelines. The actual installed
working directory is enforced, and archive/source/installed bytes match.
Pre-existing numerical modules remain byte-identical to the fitting archive.
Proofs are `evidence/installed-dxf-coordinates-verification.json`,
`evidence/dxf-coordinates-package-archive-verification.json` and the canonical
package proof. Complete DXF serialization, EMF, decoding/preprocessing,
automatic classification/count selection, document edits and the GUI remain
unfinished. Publication as ambivalence remains authorized for completion.

## Complete DXF serializer checkpoint

The independent DXF writer now matches every byte of all 65,934 successful
original exports over 3,264 fitted graphs. Splines, sparse lines and dense lines
preserve original record order, layer tables, full palette state, relative
handles, alpha acceptance, layout, visibility and coordinate fields. Thirty-six
hidden extreme files remain exact. All 144 explicit visible overlong-field
diagnostics pass; those diagnostics are distinct from successful native files.

The first implementation failed on a reversed curved edge. Exact dispatcher
assembly at 0x4741b0 establishes that callbacks receive already oriented control
points plus a separate combined direction flag. Native sampling observes both.
The canonical walker and all five writer callbacks now follow that contract.
No input-specific correction or per-format traversal was introduced. All prior
SVG, EPS, PDF and AI file corpora still pass unchanged.

Thirty additional original infinity/NaN input pairs verify derived coordinate
arithmetic, signs, quieting, payloads and actual emitted fields. Existing finite
coordinate and precision-sixteen comparisons remain exact. The retained full
native DXF fresh-process repeat still applies: the original executable, helper,
request stream and full-file fixture did not change during this implementation.

The public entry point is ambivalence/dxf and the CLI is export-dxf. Both require
the retained normalized float32 BGRA palette. Dense lines are the default;
explicit sparse-lines and splines modes are available. File/stdin CLI tests
cover 38 native files, including distinct curve modes, unused palette entries
and hidden extreme geometry. All 15 retained public prepared pipelines compare
all five formats using the actual segmentation palette and shared session state.

The inline ce-simplify-code pass read all three rubrics in one context, without
independent corroboration. Bash was unavailable; bootstrap ran through Node.
Two quality changes clarify curve direction and native no-op callbacks. One
efficiency change caches entity attributes once per shape. Broader changes were
skipped, and no unreachable code was found. Lint/typecheck are not configured.
All 49 scoped tests passed before that final review; all 175 final source tests
pass with zero failures/skips in 612,262 ms.

The DXF checkpoint archive has 35 files, 15 public entry points and zero runtime
dependencies. Its 87,158 bytes have SHA-256
`bd24ebc0ed467d2d2742828cdf5737ff401030fd544d7a5e60300b8bae438d0b`.
A fresh offline install at D:\migracion\ambivalence-verification\dxf passes all
five full file corpora, coordinate/decimal probes, diagnostics, pipelines and
CLIs. The verifier enforces that actual installed working directory and compares
all source/archive/installed bytes. The real Windows command shim also passes.
All 43 original installation files remain unchanged, all 13 contracts and ten
Python probe sources parse, and strict x86 native syntax checking passes. The
approved compiler smoke-test deletion remains in effect. Proofs are
evidence/installed-dxf-verification.json,
evidence/dxf-package-archive-verification.json and evidence/dxf-final-integrity.json.

REA identifies the EMF wrapper at 0x47cc90, constructor at 0x477cb0 and nine-slot
writer table at 0x8ddae4. Controlled binary output recovery is next. EMF, full
image decoding/preprocessing, automatic classification/count selection,
unsupported numerical modes, document editing and the GUI remain unfinished.
The parity goal remains active, and GitHub publication awaits its completion.

## EMF reference corpus and integer coordinates

Mode 58 now captures the original EMF writer after the same original fitting,
shape assembly, layout, stroke and visibility preparation as the other formats.
Read-only GetGuiResources measurements preserve all 60 initial file bytes and
show that the original retains brushes/pens across exports. That process grows
from zero to 54 objects. Full capture uses fresh processes with at most 2,048
possible new objects and 128 files per batch, without changing original calls
or deleting its objects. The recorder refuses to overwrite existing evidence.

All 22,014 files over 3,264 fitted graphs were captured in 656 bounded batches.
Every batch runs twice and its entire raw response repeats without a mask.
All request/response/repeat sizes and hashes are independently rechecked. The
ordered raw responses total 377,026,860 bytes with SHA-256
`6a4b5b3319e5094ccbb47c88bb214873e2eaeba6090e712734d0d0e8bbe20897`.
This digest includes all process prefixes and boundaries; it is not a response
from one process. File contents total 105,432,740 bytes. The 23,484,764-byte gzip
fixture retains every byte through Latin-1 and has SHA-256
`ed86fe0d33cf698619ca4845434a23177476b4ca59e94857e82c65895a238475`.
Every fitted graph equals its earlier native PDF counterpart. Ninety changed
output-dimension cases preserve complete file bytes for the same graph/options.

Headers retain device dimensions and a 64-character UTF-16 description that
continues into adjacent original static data after the intended label. All
metadata bytes repeat and remain in the reference. The device profile is
1920x1080 pixels, 527x296 millimeters and 527000x296000 micrometers. Environment
behavior remains a distinct reconstruction boundary. Microsoft specification
and API sources are linked in reference-probes.md; provider results are saved
in evidence/emf-spec-research.json.

Mode 59 captures original point helper 0x4780f0 and its scale constant from
0x8ddb08. All 277,909 coordinate pairs repeat exactly and match the independent
integer-coordinate kernel. They cover retained fitted points, every finite
exponent, adjacent 16/32/64-bit boundaries, signed zero and nonfinite bit patterns.
The native operation truncates into signed 64-bit storage, then uses its low
32 bits; overflow/invalid conversion gives a zero low word. The verified naive
implementation needs no extended-arithmetic replacement for these integer
results. It remains internal while complete EMF serialization is unfinished.

The first direct-rounding and floor/ceiling bounds hypotheses fail retained
cases. Mapping through 28.4 fixed point before integer rounding matches all
490,530 filled-path bounds. All 230,892 cubic records use the empty bounds
sentinel. Nearest header-frame conversion matches all 22,014 files. Double and
float32 arithmetic candidates are indistinguishable on this corpus; expanded
controlled GDI probes remain necessary. These are analysis results, not a
complete independent serializer claim.

The inline ce-simplify-code review used all three full rubrics in one context
without independent corroboration. Bash was unavailable; Node bootstrap was
used. Its complete output was retained, although its command was bundled with
checkpoint annotations instead of run alone. Two quality changes centralize
operation selection in the request writer and check retained counts before
writing artifacts. No reuse or efficiency change was needed; three changes
were skipped. No unreachable code was found. All initial request bytes and
parsed captures remain unchanged. Lint/typecheck are not configured.

All 176 source tests pass with no failures or skips in 604,312 ms. The extended
helper preserves complete prior SVG/PDF/AI/DXF, precision-sixteen, DXF coordinate
and pipeline responses; EPS differs only in its 41,010 actual CreationDate
fields. Fresh offline package checks pass all previous export corpora, every
new coordinate pair, diagnostics, CLIs and all 15 retained public pipelines.
The actual installed working directory is enforced and the Windows command
shim passes. The EMF-coordinate checkpoint archive has 35 files, 15 public entry points,
zero runtime dependencies and 87,412 bytes. Its SHA-256 is
`8bfa28ed62dd809aa6b1af0730b8da7308da54a0d6e26c48f471479fa98d2299`.
Proofs are evidence/emf-full-fixture-integrity.json,
evidence/emf-coordinates-native-preservation.json,
evidence/emf-coordinates-package-archive-verification.json and
evidence/emf-coordinates-final-integrity.json.

Complete EMF records/API/CLI, remaining image-processing modes, automatic
classification/count selection, decoding/preprocessing, document editing and
the GUI remain unfinished. The full parity goal remains active. GitHub
publication as ambivalence remains authorized after completion and verification.


## Complete EMF serializer checkpoint

The independent EMF writer matches all 22,014 original files over 3,264 fitted
geometries and all 402 additional wide-curve/source-dimension outputs. The latter
include 48 32-bit cubic records. Both compact and wide records preserve empty
bounds, path ordering/direction, visibility, color acceptance, grouping comments,
object IDs, source extents, header fields and the observed description bytes.
The public entry point is ambivalence/emf and the CLI command is export-emf.
Both use an explicit referenceDevice object with pixel, millimeter and micrometer
pairs. No original application or GDI implementation is used at runtime.

Mode 60 distinguishes the device arithmetic using 93,880 complete original GDI
backend cases, including every signed 16-bit value, large coordinates and
constructed half boundaries. Logical inputs, matrix scales and products round
to float32. Conversion to signed 28.4 uses nearest with ties away from zero;
pixel rounding then uses (fixed+8)>>4. Header conversion wraps bound*100 to int32
before MulDiv with physical millimeters and device pixels. Its nearest half
rounding is away from zero. All device and frame results match exactly. Earlier
failed hypotheses remain evidence, with no input-specific corrections or
runtime fallback. Primary Microsoft API sources are linked in reference-probes.md.

Mode 61 substitutes explicit source dimensions only after original fitting and
otherwise retains original assembly/dispatch. All 402 complete raw outputs
repeat exactly and match the independent writer. Ninety earlier output-dimension
changes also preserve file bytes. Window/viewport records use scaled source
dimensions. Thin stroke changes pen records although original shape end fills
paths. Out-of-range 28.4 mappings and unrepresentable 16-bit drawing-object counts
produce explicit diagnostics. System-resource exhaustion and automatic
application environment acquisition remain distinct recovery boundaries.

The original full JSON fixture exceeded Node's string limit. The canonical
reference is now one gzip NDJSON stream with indexed geometries before use,
complete case bytes and a terminal summary. Its 23,476,873 compressed bytes
retain every original file byte, with SHA-256
`b296f299ef6ee8a9957d0560c17123d968cae8ec18df78fbde08635a24cbd532`.
The original compressed JSON remains under ignored evidence. The completed
helper preserves every raw response in all 656 original batches, including
prefixes and GDI counters. All 12 retained native capture families also remain
unchanged, with only actual EPS CreationDate payloads masked for that format.

The three ce-simplify-code rubrics ran inline in one context without independent
corroboration. Bash was unavailable; the complete Node bootstrap ran alone.
Two quality changes make the 24-case CLI reference reproducible and replace
nested family expressions with explicit branches. One efficiency change reads
only the fixed native header when observing its device profile. Three broader
changes were skipped. No code became unreachable. Small generated reference
remnants remain investigation data; no evidence bytes were deleted. Lint and
typecheck are not configured. Five corrupted Bezier labels in this report were
also repaired without changing technical claims.

All 36 final scoped tests pass in 10,997 ms, including complete EMF corpora,
277,909 original integer-coordinate pairs, device boundaries, API ownership,
errors/defaults, 24 binary CLI cases and 15 retained public pipelines through all
six formats without seed overrides. All 183 source tests pass without failures
or skips in 601,319 ms. Fresh offline installation enforces its actual external
working directory and verifies every source/archive/installed byte, all six
export corpora, prior kernels/CLIs and the real Windows launcher. The archive
has 36 files, 16 public entry points, zero runtime dependencies and 90,090 bytes.
Its SHA-256 is
`694f9d149ba1412889e3c20ff06e4c8df3afc1ead74e30110e762a2185cc1b65`.
Proofs are evidence/emf-package-archive-verification.json,
evidence/emf-native-preservation.json,
evidence/emf-complete-native-repeatability.json and evidence/emf-final-integrity.json.

Prepared-image palette-count analysis now executes independently through the
2..12 candidate range, pre-quantization scoring, all 62 features and the original
model's decoded label. The predictor is 0x4abf40; 0x490dd0 decodes its label.
The decision tree matches 31,604 original vectors and ten profile names.
The prepared-image corpus retains 576 cases and 6,336 original candidates.
All 270 completed feature vectors match every double bit, including native
signed NaNs. Another 306 inputs have no histogram peaks and fault at 0x48cac2;
the library reports that condition after consuming the same random values.
Original raw-kernel captures independently verify every candidate score,
assignment, membership weight, random state and finalized palette.

An early harness omitted original neighbor-direction preparation 0x472cb0.
Its transparent-gradient score exactly matched unweighted scoring, while an
original supplied-palette call with prepared directions matched the independent
weighted score. Those recordings remain in the diagnosed uninitialized-direction
archive. The corrected harness initializes the original directions before
candidate generation; original processing instructions remain unchanged.

The independent session builds the histogram once and uses one refinement,
scoring and quantization sequence. Existing fixed-count generation and public
scoring share those kernels. CoreSession.analyzeColorCounts returns candidates,
peak weights, features and a decoded classification. Final document selection,
caches, automatic image type/quality, decoding/preprocessing, unsupported modes,
editing, environment acquisition and the GUI remain unfinished. The full goal
remains active; GitHub publication as ambivalence awaits completion and verification.

The completed palette-count checkpoint passes all 188 source tests without failures
or skips in 598,872 ms. The fresh offline installation verifies all six exporter
corpora and the 15 retained public pipelines, plus 31,604 model vectors and
576 color-analysis outcomes. Every source/archive/installed file matches. The
package has 37 files, 16 public entry points, no runtime dependencies and 92,765
bytes; SHA-256 is
`9d301c83a22e68bbfe10aa775fc974b516ac1c62c788e042c18903b511f7d2f6`.
All 43 original installation files remain unchanged. All 14 contracts and
27 Python tools parse; the native helper compiles and links with strict warnings.

The ce-simplify-code rubrics ran inline in one context without independent
corroboration. One quality fix rejects unknown capture-driver arguments. No
newly unreachable code was found. Lint and typecheck are not configured.
Checkpoint proofs are evidence/classification-final-integrity.json,
evidence/classification-simplification-review.json and
evidence/classification-package-archive-verification.json.

Further read-only recovery found the separate image-type/quality controller
0x4933e0, feature producer 0x491d40, predictor 0x4ac470 and decoder 0x490cc0.
Its predictor consumes 20 float32 features at owner +0xd8 through +0x124.
These functions are captured; independent image-type/quality parity remains
unimplemented. The latest REA bundle is
evidence/rea-classification-recovery-evidence-bundle.json.

## Image type and quality model checkpoint

The independent predictor matches all 57,472 original float-feature/extension
cases, including 65 original threshold comparisons, float32 neighbors, nonfinite
bit patterns, all five emitted labels and 22 literal extension strings. One
branch treats .bmp, .jpeg, .png and .tiff differently from .jpg, .tif, uppercase
spellings and other strings. Native type codes are photo=0, logo_aa=1 and logo=2;
quality suffixes 0/1/2 map to codes 2/1/0. The private module exposes no public
image-file classifier.

The original feature producer requires populated Magick image caches and initial
connected-region moments. The reference probe now follows the original BGRA
adapter, image setter, working-image preparation and classification-thread
region preparation before calling that producer. Its 96 repeated cases retain
72 completed 20-feature vectors and 24 empty native normalization outcomes.
Original mode 1 applies EnhanceImage, which returns null for width or height
at most four. Mode 0 is the original no-op control. These explicit controls and
fresh constructor parameters do not establish GUI defaults or all loading
behavior.

Both native gradient maps and feature slots 0/1 match independently for all
72 completed cases. The feature producer frees these maps, so the probe records
a separate original blur/gradient kernel repeat after capturing the complete
feature result, verifies no PRNG use and restores the empty gradient cache.
The original gradient-only checkpoint consumed prepared and blurred rasters.
The later prepared pipeline below supplies its own blur and all twenty features;
file preprocessing, final selection/cache behavior and complete automatic
image-file conversion remain unfinished.

At the historical image-classifier package checkpoint, seven scoped tests
passed after simplification, including the five prior
palette-count tests. The change adds one private source module; all 37 files
from the preceding installed checkpoint remain byte-identical. The fresh
offline package passes every prior output corpus/CLI, all 15 public pipelines,
57,472 image-model cases and 72 gradient/RMS cases from its external install
directory. All 38 source/archive/installed files match. The archive contains
94,586 bytes with SHA-256
`3dcfcd5e479ea610c90f4b73aee4b8a67b73add9488fe38676c750abf399075c`.
The earlier full suite passed 188 tests; this checkpoint makes no new full-suite
claim for the two added tests.

The three simplification rubrics ran inline in one context without independent
corroboration. One efficiency change removes per-pixel neighbor-array allocation
while preserving the native distance grouping. No unreachable code was found;
lint and typecheck are not configured. The original installation remains
unchanged. Full 1:1 application parity and GitHub publication remain unfinished.

That historical checkpoint confirmed all 43 installation files unchanged,
14 valid contracts and 29 valid Python tools and strict native compilation/linking. No compiler
smoke-test file is present. Proofs are
evidence/image-classifier-final-integrity.json,
evidence/image-classifier-package-archive-verification.json and
evidence/image-classifier-simplification-review.json.


## Prepared image classification and integrated shadows

`src/image-pipeline.mjs` now composes one independent prepared type/quality
analysis. All 150 original completed cases match every feature bit, label,
classification code, blur byte, gradient, contrast byte, region label and
ordinary moment. Native intermediates are comparison outputs only. The
segmentation stage supplies moments it already computes, preserving the public
four-field segmentation result without a second statistic pass.

Initial alpha preparation uses the original double multiply/truncation order.
Fixed classifier blur uses a 5x5 Gaussian with float32 exponent and accumulator
stores, double kernel weights, clamped edge coordinates and unchanged alpha.
All 2,224 explicit native rasters match, including all 65,536 alpha/color-byte
pairs and 290 one-dimensional inputs. The 288 hardware-captured kernel entries
repeat exactly and leave full untraced classification payloads unchanged.

Shadow detection, 42-feature production and refinement now run in canonical
prepared segmentation. The 640 explicit refinement cases include 7,600 flagged
regions and 24 rejected changes. Complete dispatch also matches 578 native
automatic/supplied-palette results across cluster states, five profiles and four
acceptance controls, including 81 accepted and six rejected shadows. All 72
constructor-context maps/moments now pass the previously failing comparison.
The context recorder preserves both the unregistered shadow switch and its
separate int32 control at core 0x298; the serialized image-type code is a
different field.

The original first PCA scalar is uninitialized. Canonical zero matches every
recorded observable feature and direction byte; explicit retained-state kernel
comparisons remain intact. This is bounded evidence, not proof of original
zero initialization or every possible allocator state. Full image-file
conversion still needs decoding, other preprocessing modes, large-image
reduction, GUI profile/default resolution, document/cache/editing workflows and
the custom GUI. Publication remains gated on the user's complete 1:1 outcome.

## Enhance preprocessing

The independent private `src/image-preparation.mjs` implements EnhanceImage and
mode-one preparation over decoded BGRA. The original uses a weighted 5-by-5
neighborhood, an opacity-dependent double color distance with a strict 2601
cutoff, float32 accumulation and biased integer normalization. Replacing those
operations with ordinary averaging or rounding changes bytes. Dimensions below
five produce the original empty image, including during repeated passes.

All 4,208 direct native kernel cases and 1,616 whole preparation cases match
exactly, including 1,728 cutoff-adjacent inputs, borders, transparency, unchanged
caller pixels and zero through three passes. Each full native response repeats
identically without patching original instructions. The whole preparation
comparisons also verify subsequent independently generated Gaussian outputs.

The fresh private 41-file package installs offline outside the repository.
All archive/source/installed bytes agree, and the previous export corpora,
15 retained-session pipelines and new preparation cases pass from that install.
A restricted installed process also passes while original-file reads and child
process creation are denied. The three simplification rubrics ran inline with
no independent reviewer corroboration and found no useful changes. No code
became unreachable. Lint and typecheck commands are not configured.

REA queries 680 through 684 recover histogram and hull helpers for the remaining
filters. Investigative naive code matches 256 mode-two and 96 mode-three cases.
Those filters are not shipped yet. Small mode-two images can raise a first-chance
Magick::ErrorOption exception; the development probe stops before application
handling, so its final UI/error outcome remains unresolved. Full application
parity and publication remain unfinished.

The current full source suite passes all 206 tests with zero failures, skipped
tests or cancellations (601181.4521 ms). Reports are
`evidence/image-enhance-full-test-run.txt`,
`evidence/image-enhance-package-archive-verification.json`,
`evidence/isolated-image-enhance-verification.json` and
`evidence/image-enhance-final-integrity.json`. All 43 installation files are
unchanged; 39 Python tools, 15 contracts and 37 package modules pass syntax
validation. The archive has 105129 bytes and SHA-256
`dffdeb03caf57e95fa7d50302912ab4b442d81a1848dc080df37b0bd58ac2249`.

## Complete explicit preprocessing dispatch

The private preparation module now implements the nonpeak median and hull
despeckle filters. One dispatcher premultiplies decoded colors once and applies
the native mode/amount behavior: mode zero keeps the pixels, mode one repeats
enhancement, mode two selects a 3/5/7 median kernel, and mode three runs one
despeckle regardless of amount. Large-image working resize remains separate.

Native comparisons cover 10,186 direct filter cases and 3,839 complete dispatch
cases, including exhaustive three-level histogram populations, binary 3-by-3
fields, border handling, opacity and single-row/column inputs. Every successful
native response repeats exactly. All 176 median size errors preserve the native
ErrorOption type, message and stage in reference evidence. The private JavaScript
kernel reports the same ImageSmallerThanKernelRadius reason as RangeError;
complete application error presentation is still unfinished.

Another 123 complete native classifications through modes two and three match
independent decoded-pixel preparation, all 20 feature bits, final labels,
connected moments, blur, gradients and contrast. The total prepared-classifier
coverage is now 273 cases, with 195 including initial decoded-pixel preparation
and 78 retaining explicit prepared-input scope. None require captured native
intermediates as runtime inputs. Source syntax and native precision assertions
remain strict.

The simplification review ran its three rubrics inline, with no independent
corroboration. The median implementation was first verified as a direct sorting
algorithm, then changed to gather all four channels in one neighborhood pass.
A small 30-run 64-by-64 radius-three measurement changed from 220.37 ms to
196.18 ms; this is a local measurement, not a general performance guarantee.
All seven preparation tests still pass. No code became unreachable.

The complete current suite passes all 211 tests with no failures, skips or
cancellations (560219.1216 ms). The fresh private package contains 41 files and
106431 bytes, SHA-256
`ec8798cec4dcea839073e08d303fb3044081f82f14770e9da16b4f99bbf5825f`.
All archive/source/installed bytes agree. The restricted installed runtime
repeats all 273 complete image analyses, old prepared segmentation/blur/Enhance
comparisons, 14025 new filter/dispatch cases and 176 size failures with original
file reads and child processes denied. All 43 installation files are unchanged;
40 Python tools, 15 contracts, 37 package modules and two verification scripts
pass syntax checks. Reports are `evidence/image-filters-full-test-run.txt`,
`evidence/image-filters-package-archive-verification.json`,
`evidence/isolated-image-filters-verification.json` and
`evidence/image-filters-final-integrity.json`. Full conversion, resizing, file
decoding, document/profile/UI integration and publication remain unfinished.

## Q8 resizing and large-image working reduction

`src/image-resize.mjs` independently reproduces the original sampling and
triangle-filter branches, native Geometry sizing and post-preprocessing working
reduction. It preserves source pixels and retains Q8 quantization between filter
passes. The implementation follows the original mixed float32/double stores,
including different spill positions for matte opacity, alpha sums and each
non-matte color channel. Ordinary per-tap float32 summation changes rare bytes.

The Geometry producer omits dimensions that round to zero, so a one-pixel-wide
image can request a height-only fit. The two-axis fit retains a double width
ratio and a float32 height ratio; one-axis fitting stores the requested integer
as float32 before division. A literal zero-width string would model the wrong
producer. Native text serialization is explicitly checked during capture.

All 12,084 Q8 resizes, 20,056 geometry fits and 92 complete working preparations
match exactly. Coverage includes the 200,000-pixel threshold, sampled/filtered
paths, matte flags, up/down/same-size resizes, long thin images, all preprocessing
modes, four empty results and 70 actual reductions. The Gaussian/resize kernels
do not consume random values, and source/working cache ownership is preserved.
The triangle leaf was not recognized as a procedure by REA, so its original
assembly supplied the arithmetic; other recovered functions use REA dossiers.

The three simplification rubrics ran inline with no independent corroboration.
One shared float64 scratch buffer makes the channel-specific storage points
explicit and removes per-pixel allocation. Indexing taps directly also removes
temporary iterator entries. Every scoped comparison remains exact; no code
became unreachable. Lint and typecheck commands are not configured.

At the resize checkpoint, the full suite passed all 214 tests with no failures,
skips or cancellations (668794.0395 ms). That private archive contains 42 files and
108468 bytes, SHA-256
`2c2abab30d0d10765ccede18c205fbf40b2159d2a4008a8675eaef9ea2385df1`.
Every source/archive/installed byte agrees. The installed exporter/pipeline
checks and restricted-runtime comparisons include every new resize case.
Reports are `evidence/image-resize-full-test-run.txt`,
`evidence/image-resize-package-archive-verification.json` and
`evidence/isolated-image-resize-verification.json`.

REA and five additional native gradient captures show that classification reads
the retained full source blur and the reduced working image with separate row
strides. The type/quality controller then resolves its label, updates anti-alias
state and prepares the working image again before palette-count analysis.
Further REA inspection shows that 0x491ce0 stores a label-associated registry
value at the shared-settings object's +0x14; that function alone does not apply a
complete settings profile. This stateful orchestration, file decoding, complete
document workflows and the GUI remain unfinished.

## Fresh source and working classification

The canonical private image state owns the full preprocessed source and its
working reduction. Classification and shadow segmentation retain both rasters;
gradient reads reflect at working bounds while using the full source blur's
physical row width. Standalone prepared segmentation explicitly uses its raster
in both roles. The earlier private classifier shape has no retained alias.

Mode 90 records 16 complete original analyses over solid/four-color block images,
transparency, both anti-alias controls and preprocessing modes 0/2/3. Twelve
working images are reduced. Both 85,587,969-byte recordings match exactly.
Independent preparation and classification match every source/working byte,
20-feature bit pattern, classification decision, full source blur, working
gradient map, contrast byte, region label and ordinary moment. Five additional
distinct-stride gradient states also match. Native intermediates remain test
expectations, never runtime inputs.

All 216 source tests pass with no failures, skips or cancellations
(596991.5067 ms). The current offline-installed archive contains 42 files and
109062 bytes, SHA-256
`4652e9d68d7f48bb026071fd93e3470ec7d677f75adcb8a2187272b948139d6e`.
All source/archive/installed bytes agree. The restricted installed runtime also
passes the new state comparisons with original-file reads and child-process
creation denied. All 43 original installation files remain unchanged.

The ce-simplify-code rubrics ran inline without independent corroboration. One
recording palette moved outside the pixel loop; regenerated request bytes remain
identical. No code became unreachable. Lint and typecheck are not configured.
Current proofs are `evidence/image-state-full-test-run.txt`,
`evidence/image-state-package-archive-verification.json`,
`evidence/isolated-image-state-verification.json` and
`evidence/image-state-final-integrity.json`. The authoritative fresh-state
contract is `docs/image-state-contract.json`.

REA 710 through 717 additionally recover the Shared label registry and source
preprocessing cache flag. Labels logo 0/1/2 map to codes 0/1/2, logo_aa 0/1/2 to
3/4/5, photo 0/1/2 to 6/7/8, and classify to 9. The source setter clears the
preprocessing flag; applying modes 1/2/3 marks it to prevent another filter pass
when working preparation repeats. These observations guided the complete
controller capture below. File decoding, document/edit/cancel workflows,
unsupported numerical modes and the GUI remain unfinished.

## Complete fresh automatic image analysis

`CoreSession.analyzeImage` now joins independently prepared decoded pixels,
type/quality classification, Shared image-type/anti-alias updates, repeated
working preparation and palette-count analysis on the existing session PRNG.
The filtered source survives the second working preparation. Raw palette scores
remain available for feature inspection; separate recommendation scores match
the later native cache writes. Returned parameter arrays own their storage, and
palette controls cannot override the working raster.

Mode 91 calls the complete original 0x4933e0 controller after classification-thread
segmentation. All 42 completed inputs and two native empty-peak faults match
across three retained process sessions. Comparisons include all image and
palette feature bits, both decisions, source/working bytes, retained candidate
palettes and weights, recommendation fields, the final assignment map and every
observed random transition. Inputs cover all four preprocessing modes, all five
emitted image labels, all three palette classes, 18 anti-alias changes and eight
working reductions. Independent naive composition passed before API integration.

The original empty-peak cases fault at 0x48cac2 after candidate generation. Their
semantic payloads and fault code/instruction/stage repeat exactly, while raw
exception register pointers remain volatile evidence. Each fatal ends its
native process session. The library reports its explicit empty-peak diagnostic
and preserves the captured random effects. No per-stage seed override or native
intermediate input is used.

The `analyze-image <image.json|->` CLI command uses the same controller and emits
JSON classifications, resolved type/AA fields, working dimensions, candidate
palettes/weights/errors/recommendation scores and final random state. Native
fresh-session comparisons cover stdin and a path with spaces, plus help,
malformed input and empty-peak diagnostics without partial output.

All 221 source tests pass with no failures, skips or cancellations
(612578.9197 ms). The current private archive contains 42 files and 110658 bytes,
SHA-256 `b806f99bc992c19d7db3a15d2a48933ff2727f275f387cc6cdd28de74c57eb7c`.
Every source/archive/installed byte agrees. The expanded CLI is intentionally
different from the old palette checkpoint; 31 other files retain historical
pins. Installed API/CLI comparisons and the restricted runtime pass, including
all 44 controller outcomes. Original-file reads and child-process creation are
denied in the restricted runtime. All 43 original installation files are unchanged.

The create-cli and ce-simplify-code skills guided the interface and review. The
three simplification rubrics ran inline without independent corroboration; no
further refactor was needed and no code became unreachable. Both recorders share
native-context decoding, with all sixteen older contexts replayed unchanged.
Lint/typecheck are not configured. Current proofs are
`evidence/image-controller-full-test-run.txt`,
`evidence/image-controller-package-archive-verification.json`,
`evidence/isolated-image-controller-verification.json` and
`evidence/image-controller-final-integrity.json`.

Complete controller captures use the original BGRA adapter's matte-one source
context. Original file/non-matte decoding and extension normalization remain
upstream recovery work. REA 723 through 726 locate the actual encoded-image
entry at 0x473e00: construct a blob through 0x4aee70, read it through 0x4ae6d0,
then pass the decoded image through source setter 0x473bb0. The next native probe
must record raw decoded bytes, matte/alpha state and post-setter source cache
before implementing file loading. Final document color-count choice, GUI
defaults, edit/cache/cancel lifecycle and the GUI remain unfinished. No GitHub
or npm publication has occurred.


## Palette document progress, cancellation and caller retry

The independent document now exposes checkpoint progress and requestCancel().
Original instructions establish initialization before callbacks, exact progress
fraction/percent order, partial candidate commits and retained random effects.
Cancellation throws CancelTaskException without committing the requested
settings. A caller can catch it and retry the same document. Earlier cached
palette behavior remains covered by 220 edits and 21,142 exact formatted keys.

The new native corpus has 234 cases and 498 steps: 354 completions and 144
cancellation exceptions. It records 9,616 progress events. A development-only
outer C++ catch verifies 48 original stack unwinds and subsequent same-document
retries. Direct original-task and actual Qt-thread captures retain their distinct
notification schedules. The original controller uses thread priority 2. Actual
GUI cancellation completion and late status 2 remain unverified; the outer
caller catch is not a replacement for a GUI handler.

The canonical development build uses ordinary COFF input/subsection ordering,
with original addresses reserved first and NO_SEH clear. No generated object or
original instruction rewriting remains. All 234 final case frames match the
scoped and installed expectations exactly. Replayed 636 background cases and
220 palette actions retain every engine-result byte. Their only raw recording
change identifies the helper's own entry address, recorded explicitly as probe
provenance. All 43 original installation files are unchanged.

The complete source suite has 228 tests: 227 pass, one known encoded-PNG failure,
zero skips (616625.6324 ms). The failure remains adam7-9x7-10 decoded; no expected
bytes or assertions changed. The private 44-file archive is 118980 bytes,
SHA-256 6cb3b3377ccc49c8fe3cc40330a98a8380d0b35a5681f51f6a3fdca2df974971.
Source/archive/installed bytes agree. The isolated installed package passes all
new steps and progress events, the 220 earlier actions and 21,142 keys with
original-file reads and child creation denied. No runtime dependency was added.

The ce-simplify-code lenses ran inline, without independent corroboration.
One quality simplification removed generated-object rewriting; existing shared
state decoding/build/verification and callback efficiency were retained. No
source became unreachable. Lint/typecheck are not configured. Proofs are
`evidence/document-cancellation-full-test-run.txt`,
`evidence/installed-document-cancellation-verification.json`,
`evidence/document-cancellation-package-archive-verification.json` and
`evidence/document-cancellation-integrity-verification.json`.

The GUI contract preserves Vector Magic workflow/control grouping with
shadcn/ui controls, updated icons and original ambivalence branding. GUI task
references are recorded, but the GUI remains unimplemented. Full conversion,
remaining decoding/numerical/document workflows and 1:1 application parity
remain unfinished. No GitHub or npm publication has occurred.


## Document segmentation capture and settings field identity

Operation 100 constructs the original document and runs 113 palette/segmentation
actions on four decoded sources, including 512x512 input. Complete fresh native
responses repeat byte for byte. The corpus contains all 100 typed parameters,
source/working pixels, stage keys, candidate caches, committed palette arrays,
region maps/colors/counts and random state. It covers fixed, two-color and
flexible modes, all basic profiles, advanced controls and interleaved requests.

Complete profile captures exposed a wrong field identity in the resolver.
Original +0x214 controls shadow dispatch; registered
Segmenter::use_over_seg_diag_extr lives at +0x930. The earlier arithmetic reader
called +0x214 overSegmentation. Advanced artifact rejection now returns a
separate shadowing boolean and preserves the diagonal parameter from profile 5.
All 648 original raw arithmetic records and numeric values remain identical;
72 semantic labels are corrected. A new comparison first failed on that exact
registered field, then passed all 11300 parameter byte comparisons after the
resolver correction. No expected numeric values were relaxed or changed.

Independent naive preparation, state-switch resolution and the existing
segmentation engine match all 81 native recomputations, all 20 key/cache
decisions and retained-session random effects. This composition computes its
own pixels and parameters. It does not inject native intermediates. The source
script and exact result are retained in
artifacts/document-segmentation-profile/naive-document-segmentation.mjs and
evidence/document-segmentation-naive-comparison.json.

Native palette work writes the same live label grid and palette used by
segmentation. It can preserve the segmentation key and old region metadata.
After a range expansion, the large case has a 387x387 six-color assignment while
retaining 262144 pixels of earlier region counts. A key hit exposes that shared
state. An immutable cached segmentation would be the wrong design. Independent
live-buffer cache-hit integration and a public document segmentation method
remain unfinished. Segmentation uses full source preparation on a key miss;
basic/palette stages preserve hidden shadow/clustering switches written by a
prior advanced stage.

The full source suite has 229 tests: 228 pass, one existing encoded-PNG failure,
zero skips (606834.064 ms). The failure remains adam7-9x7-10 decoded. The new
private 44-file archive is 119125 bytes, SHA-256
3b17066fb2e7baf1cb7dee0e48bae78d8fbbbf21fee385558322af5c9316cd4b.
Source/archive/installed bytes agree. Only settings.mjs and README.md differ
from the prior archive; 42 files retain their pins. The installed resolver
matches all 11300 parameter comparisons and 648 arithmetic cases with original
reads and child creation denied. Native regressions preserve every engine byte
for 636 background cases, 220 prior actions and six cancellation contexts.

The ce-simplify-code lenses ran inline with zero independent contexts. One
quality simplification made test control branches explicit; no source became
unreachable. Lint/typecheck are not configured. Current evidence is
recorded under document-segmentation-*.json and
installed-document-profiles-verification.json. Full conversion, remaining
numerics/decoding/document workflows and the shadcn/ui GUI remain unfinished.
No publication has occurred.


## Unified document state, live caches and CLI shadow context

The canonical ImageDocument replaces the palette-only document model. Its
CoreSession.createDocument factory owns decoded pixels on one retained session
PRNG. Palette analysis and segmentation take the full explicit verified
settings and share native live assignment/palette state while preserving older
region metadata when the original does. No compatibility factory is retained
in this private unreleased package.

The extended native operation 100 corpus contains 145 actions on four sources:
16 palette requests, 93 segmentation recomputations and 36 cache hits. All
14500 registered typed parameter values, hidden switch lifetimes, full pixels,
settings, assignments, palettes, region metadata, stage keys and PRNG match.
Supplied colors transfer before key comparison, including hits. Adjacent
float32 colors with the same formatted key preserve the native cache decision;
flexible mode ignores requested palette colors in its segmentation key. The
large case can retain old full-resolution region metadata beside a reduced
live grid. Native intermediates remain expectations only.

REA 809..827 establishes corrected document key identities: +0xf8 segmentation
edits, +0x114 smoothing and +0x130 fitting. The earlier decoder's names were
wrong; native response bytes and all state values are unchanged. +0xc0 remains
explicitly reserved until its role is established. Smoothing and fitting
wrappers, success continuations and settings-key dependencies are now traced,
but their complete document actions still require executable comparison.

The prepared segmentation CLI previously dropped a supplied shadowControl
when resolving advanced settings. It now forwards that explicit context to
the canonical segmentation boundary. The regression compares all 837 indices,
four native palette colors and four region records for 31x27-noisy, and keeps
the missing-context error visible with empty stdout. All 30 CLI tests pass.
This prepared-command proof does not inject native state into the decoded
ImageDocument implementation.

The final private MIT archive contains 44 files, 120867 bytes, SHA-256
9602af27921cca7ba4fb3b1688c2cafdb06c9b90b7da205f8570a2c3d515676e.
Source/archive/fresh-install bytes agree. The image-document module replaces
palette-document; README, the palette factory and CLI changed from the prior
profile archive. Forty other files retain their pins. Sixteen exports and zero
runtime dependencies remain. The installed verifier passes all 145 document
actions, 220 palette actions, 21142 keys and 234 cancellation cases / 498 steps
with original-file reads and child creation denied. The restricted installed
CLI matches the original advanced result, missing-context contract and source
help. All 43 original installation files remain unchanged.

The ce-simplify-code reuse, quality and efficiency lenses ran inline, honoring
the no-delegation instruction. Tracking the previous key avoids a full snapshot
copy before every document action in tests and verification. No dead code was
found. Lint/typecheck are not configured. The final full suite runs 231 tests: 230 pass, one known encoded-PNG
failure, zero skips (603168.4354 ms). Its exact result is recorded
in image-state-contract.json and image-document-final-full-test-run.txt. Complete file
conversion, remaining document stages/cancellation/errors, undefined native
Adam7 decoding cases and the required modernized shadcn/ui GUI remain
unfinished. No GitHub or npm publication has occurred.

## Real-widget automatic workflow pilots

A separate development helper now constructs the actual native QApplication,
MainWindow, widgets, MainModel and ThreadManager. It runs the original CRT
initializers and resource setup. The native exit registry must be initialized
as the original entry does. Recorder paths use environment inputs because Qt 4
reconstructs the actual Windows command line. No original instructions or
installation files are changed, and the component reference helper is unchanged.

Initial controls and owner links agree with read-only inspection of the live
native GUI. The initialization record repeats exactly. The actual
setImageFilename(QString) slot loads an own BMP through real queued completion;
the independent decoder reproduces its pixels with original reads and child
creation denied. The native document initializes recommendation category only;
the other three recommendation fields become valid after classification.

Three runFullyAutomatic pilots, each in two fresh native processes, repeat
exactly. Independent composition computes recommendations, applies the recovered
opaque-source background policy, chooses the palette through its QColor channel
representation and runs segmentation, smoothing and fitting. All selected
controls, color counts, candidate caches, 300 registered parameter records,
source/working pixels, complete contours, smoothing metadata, fitted geometry,
assembled chains and session PRNG match. Reference intermediates are expectations
only. The three inputs cover artwork with and without blending, and two-color
and fixed-palette recommendations.

A fourth input, an own smooth gradient, faults in the native empty-peak classifier.
The independent classifier reports its corresponding empty-histogram error.
Both failures remain visible. Complete GUI failure completion is not established.
Explicit independent shadow context 0 does not prove a native file default.
Transparent sources, repeated actions, manual edits, batch processing and
in-flight cancellation still need capture. Automatic mode remains disabled in
the independent GUI. The modern shadcn/ui preview and numerical engine were not
changed by this reference-tooling work. No GitHub or npm publication occurred.

## Retained automatic GUI session

Operation 107 now invokes twelve actions on one actual MainWindow: four opaque
BMP file loads and eight Auto requests. Both fresh session recordings match
byte for byte. Independent processing keeps one engine session and computes
all results before loading reference expectations. Every load matches selected
controls, supplied-palette clearing, original pixels, recommendation-category
reset and preserved PRNG. All eight complete processed states match candidate
caches, 800 registered parameters, source/working pixels, contours, smoothing
metadata, fitted geometry, assembled chains and the retained random stream.

Static recovery 912-916 explains the observed retained-control reset branch:
MainModel calls 0x443ff0, which clears recommendation fields and palette storage
through 0x46cd50 while preserving chosen controls and requested color count.
The wider reset method branches on a preferences-restored lifecycle flag and
an explicit reset argument. Clean-install profile behavior still needs
separate verification. Repeated Auto requests advance classification's random
stream even when later palette work can use cached candidates. A file load
preserves that stream and resets recommendation category to -1.

The earlier three fresh pilot responses retain their exact accepted hashes
after sharing GUI wire readers and the independent comparison code. Product
engine and GUI sources remain unchanged; the private engine archive's 44 files
still match their sources. Full automatic integration, transparent inputs,
manual editing, batch workflows, complete failure handling and application
parity remain unfinished. See the retained-session verification checkpoint.

## Transparent PNG automatic workflows and startup provenance

Four own RGBA PNGs cover opaque, binary-alpha and partial-alpha originals. Each
complete native file/Auto response repeats in two fresh processes and matches
independent composition through final geometry. A separate retained session
with five PNG loads and ten Auto actions also matches all loaded and processed
states, including 1000 typed parameters and the retained PRNG. Together these
add 1400 parameter comparisons without changing product source.

The binary-alpha blended-artwork case exercises the special background branch.
Native 0x429610 chooses command 3, and 0x4283a0 maps it to mode 1 with opaque
white. The other covered basic cases select mode 0 with transparent white.
Command and setting enums differ. The alpha classifier scans the prepared
original; repeated requests and source changes preserve the same policy.

REA 918 resolves MainModel +0x2dd to a flag set after 0x42f410 restores QSettings.
The earlier description as a reset preference was incorrect. Captured startup
controls match the current saved default profile, not established clean-install
defaults. A read-only whitelist of twenty vectorization controls confirms nine
native startup fields exactly. That inspection wrote no registry value and read
no license or MRU values. Recoveries 919-920 identify the profile reader and raw constructor;
operations 108/109 below establish the missing-profile values and clean startup.

Earlier BMP pilot/session responses and all original installation files remain
unchanged. Full GUI policy, source-shadow context, manual edits,
batch processing, failure handling and remaining codecs are still unfinished.
Automatic mode remains disabled, and no publication occurred.

## Constructor and missing-profile setting defaults

Operation 108 constructs actual native settings QObjects through 0x444510 and
reads absent default/batch profiles through 0x442bf0. The original x86 member
call uses hidden QString return storage, then index and group value arguments;
the callee owns and destroys both arguments. All 628 response bytes repeat in
two fresh processes. Both missing keys stay absent, the twenty inspected fields
in each saved profile are unchanged, and all 43 installation files remain pinned.

Independent ImageDocument initialization matches the native constructor's 17
corresponding controls. Constructor category is -1, quality/sensitivity 1,
complexity 5, smoothness/curve complexity 6, palette mode 1 and background mode
-1 with transparent black. Both anti-alias switches, clustering and corner
detection start enabled; palettes are empty.

The missing default profile converts its background to opaque black, sets
palette-color count 3 and recommended background mode 0. The missing batch
profile uses Auto sentinels 3 for category, quality and palette mode, with
background mode 1 and opaque black. These are settings-reader values. The
MainModel startup later forces selected palette-color count to zero at 0x43027c.
Operation 109 verifies that startup and eight Auto actions in the actual MainWindow
under a capture-owned, initially absent application preferences namespace. Four
loads, 800 typed parameter records, metadata, caches, pixels and final geometry
match independent processing. Both fresh responses repeat exactly; the owned
namespace is removed and two saved numeric profiles remain unchanged. Final
batch processing remains unverified.
The saved startup profile is not substituted for these component defaults.

The completed source suite runs 234 tests: 233 pass, one retained
adam7-9x7-10 decoding failure, zero skips (632736.4827 ms). Five durable document
tests pass against the fresh installed package with original/workspace engine
reads and child creation denied. The new private 44-file archive is 125216
bytes, SHA-256 9a519cdbf93ce4122670c3a1596796a936c08d8d497f49b263e9c6a52863cbcf.
Five shipped files changed; the other 39 preserve their previous pins. Sixteen
exports, zero runtime dependencies and MIT licensing remain. The fresh
73-file GUI ZIP is 368041 bytes, SHA-256
f6b637f06a11a5b4b96e9973e567976336cb1e1b56e5a12616a8b322463b46cb.
Its three integration tests include advanced mode with category -1. The updated
isolated preview runs on port 3742; the earlier preview session is retained.

The GUI worker permission probe preserves original-read and child-process
denial but permits workspace engine reads on Node 24.12.0. Main-thread checks
cannot establish worker filesystem isolation. This limitation and failed
attempts are retained explicitly in the checkpoint; no denial assertion was
silently treated as passing. The simplification review made no further changes,
found no newly unreachable product code and used zero independent contexts.
Lint/typecheck remain unconfigured for this JavaScript project.

REA 895-901 identifies MainModel completedTask at 0x4354c0 and prepareForTask at
0x42ce10 through actual Qt metadata. Classification completion applies separate
recommended and selected fields, with batch automatic-sentinel guards. Task
preparation dereferences the real GUI progress bar. Automatic flow must therefore
be captured with initialized native widgets; a fabricated model or null GUI
cannot establish that behavior. No automatic GUI policy has been enabled from
static inspection alone.


## Document smoothing/fitting integrated with native cache behavior

Development operation 101 records complete palette, segmentation, smoothing and
fitting document requests using the original constructed document receiver.
All 378 actions on eight decoded sources repeat byte for byte across fresh
native processes. Original instructions remain unchanged. The corpus includes
all nine basic profiles, advanced smoothing/AA/corner controls, curve-only
edits, interleaved palette work, nested shapes and a 512x512 input.

Independent naive composition first matched complete geometry, metadata,
assembled output chains, session PRNG and all 37800 typed parameter records.
The canonical ImageDocument then added smooth and fit methods that make their
own native-key decisions. All 77 smoothing recomputations / 85 hits and 113
fitting recomputations / 45 hits match. Fitting changes live contour node kinds;
a smoothing cache hit preserves those fitted markers. Fixed references and
border metadata come from the same smoothing preparation pass rather than a
second computation. The seventeen explicit settings keep palette anti-alias
separate from smoothing anti-alias. Full initial native settings establish
smoothness and curve complexity defaults as 6, correcting the initial guess
of 5 without changing any expected native result.

Nine focused tests pass, including both document corpora, palette keys,
source/cancellation state and all 15 six-format prepared pipelines. The fresh
installed package matches 523 document actions, 52300 typed parameter records,
220 palette actions, 21142 keys and 234 cancellation cases / 498 steps with
original-file reads and child creation denied. The current restricted CLI also
preserves the native advanced segmentation result, missing-context error and
help bytes. All 44 source/archive/install files agree. The private MIT archive
is 122279 bytes, SHA-256
7db4d63842bf74da715a14eb55e920166bfc070658f4f9b1a99a4409a437a80d.
Only README, image-document, palette factory and contour-smoothing metadata
output changed from the preceding archive; 40 files keep their pins.

Native capture readers now take the actual core context. The prior 145-action
document response remains byte-identical, and fresh frozen before/after helpers
agree on all 2712 prepared fitting states. The much older historical fitting
record differs only at a helper entry-address header byte, with every engine
result byte unchanged. The generic compiler build passes with working original
frame handlers and no generated-object rewriting. All 43 original installation
files remain unchanged. The approved compiler smoke test remains absent.

The ce-simplify-code lenses ran inline with zero independent contexts. No
further behavior-preserving simplification was justified. No dead product code
was found. Lint/typecheck are not configured. The final full suite runs 232 tests: 231 pass, one known
encoded-PNG failure, zero skips (599338.9222 ms). The current result
is recorded in image-state-contract.json and document-vector-stage-full-test-run.txt.

Static GUI-controller recovery distinguishes lazy transparency detection from
palette readiness and worker dispatch. Executable GUI/default/palette-choice
captures remain required. Manual edit commands, supplied vector-stage palette
edits, vector cancellation/errors, complete file/export orchestration and the
modernized shadcn/ui GUI remain unfinished. No publication has occurred.


## Completed native Qt task-controller boundary

Operation 102 constructs the actual native ThreadManager and QCoreApplication,
launches original workers and lets Qt deliver queued completion. The first
pilot lacked original dynamic Qt parent-metadata initialization; its missing
inherited finished() signal prevented completion. Original CRT stubs 0x6f8c10
and 0x6f8c90 fix that startup defect. No manual callback, fake MainModel or
instruction patch was substituted. All 384 single-stage requests on eight
sources now repeat byte for byte, with 243 computed requests and 141 skips.
Automatic continuation is explicitly false; full GUI scheduling is not claimed.

Independent ImageDocument.runTask checks keys/completion before invoking the
same validated stage kernels. It returns owned requested settings and completed
document state. Palette skips commit requests without reloading a core profile;
other skips keep prior committed settings/profile. The distinct behavior is
covered by ignored-control edits, complete live pixels/geometry/chains, native
Qt completion fields, all 38400 parameter records and session PRNG. Existing
145/378 direct-stage oracle responses remain byte-identical. The installed
verifier covers 907 actions and 90700 typed parameter records alongside palette
keys/actions and cancellation/retry, denying original reads and child creation.

The full suite runs 233 tests: 232 pass, one known encoded-PNG failure, zero
skips (634023.332 ms). No native expectations were weakened. The new private
44-file MIT archive is 123181 bytes, SHA-256
bdcb934f4e3bd2bc498be675d985253c56a9d30676601c2c2fe3f5f43682e3bb.
Source/archive/fresh-install bytes agree. Only README and image-document changed
from the prior archive; 42 files keep their pins. Sixteen exports and zero
runtime dependencies remain. All 43 original installation files are unchanged.
The approved unused compiler smoke test remains absent.

The ce-simplify-code lenses ran inline with zero independent review contexts.
One quality change makes accepted recorder modes explicit. Stage request
validation occurs once and both interfaces share private kernels. No dead
product code was found; lint/typecheck are not configured. Current verification
is recorded under task-controller-*.json and installed-task-controller-verification.

The reference window opens, but both initial and one recovered screenshot
capture time out; accessibility exposes unnamed panels. No unobserved control
clicks were made. The required GUI remains unimplemented. Embedded original
help documents supply a documented right-side wizard and viewer control flow,
while actual GUI/default behavior still needs executable verification. Automatic
analysis/palette choice, remaining edit/cancellation/error/file workflows and
full application parity remain unfinished. No publication has occurred.
# Modernized GUI preview, 2026-10-17

The first runnable GUI is in `gui/`. It uses real shadcn/ui components, Lucide
icons and an original Ambivalence mark on a black background. Its single viewer,
left-side navigation/layer controls and right-side wizard follow the embedded
reference help. Basic and advanced pages call the existing independent document
engine through a local Node host and worker. No numerical engine source changed.

The portable preview contains 73 verified files and no `node_modules`. A fresh
extraction passes all three real GUI integration tests. Original executable
reads, workspace engine reads and child creation are denied in the main thread;
a later worker probe shows these restrictions are not enforced inside workers
on Node 24.12.0. Worker filesystem isolation is not established. All nine scoped
document tests still pass; the previous complete engine suite retains its known
Adam7 PNG failure. The original installation's 43 pinned files are unchanged.

Own-GUI browser checks cover the basic conversion flow, advanced controls,
Revert/Update, named sliders, layer inspection, outlines and desktop/narrow
layouts. HTTP exports match canonical writer bytes and attachment headers.
Browser automation cancels downloads, including a fresh-browser attempt;
successful completion through the GUI download button is not claimed.

Automatic classification/default resolution, complete manual editing and batch
flows, remaining codecs, native desktop packaging and full reference GUI parity
remain unfinished. No GitHub or npm publication occurred. See
`gui-contract.json` and `../evidence/gui-preview-verification-checkpoint.json`.

## Document classification and recommendation lifecycle

Operation 104 adds real ClassifyImageThread requests through ThreadManager kind
1 and actual Qt queued completion. All 380 actions on 19 sources repeat byte
for byte. The corpus includes 133 classifications, six explicit extension
strings, source resets, background edits, advanced unclassified requests and
classification after fitted vectors. MainModel automatic continuation remains
disabled. No original instruction or intermediate is substituted.

Independent ImageDocument.classify and runTask('classification') match all
recommendations, image/palette features, eleven candidates, source/working
pixels, shared fields, 38000 typed parameter records and retained PRNG. Native
classification starts with full-size prepared pixels, then reduces them for
palette analysis using the loaded edge weights. It clears only the segmentation
key and retains candidate caches and vector geometry. Requested settings remain
separate from the four recommendations. Source reset clears only recommendation
category and retains quality, palette mode and color count.

The Shared constructor leaves source shadow context uninitialized. It is now
an explicit int32 source input, zero in this bounded corpus. Failed uncontrolled
recordings remain under document-classification-uninitialized-context-*.
Native usePalette/reserved members are byte booleans; the recorder serializes
their typed values without adjacent uninitialized padding. Failed raw-padding
recordings remain under document-classification-explicit-shadow-raw-shared-*.
The complete accepted responses remain strictly byte-identical. Replaying all
907 prior operation 100/101/102 actions preserves their accepted response hashes.

The GUI passes this explicit decoded-source context and preserves an
unclassified category when entering advanced mode. Its three real integration
tests and production frontend build pass. Automatic GUI recommendation policy,
file context/extension production, manual editing and complete parity remain
unfinished. No GitHub or npm publication has occurred.


## Flexible-color automatic continuation

A fresh real-GUI capture of the independently encoded own 31x27 noisy source
recommends [1,0,2,12]. It skips palette search and proceeds through segmentation,
smoothing and fitting. All candidate-completion flags remain clear. Independent
processing matches the complete final state, 100 typed parameters and geometry.
REA 923 confirms segmentation display synchronization does not copy its dynamic
palette back into the selected settings. A separate anchored 48x48 texture also
matches all final fields, with [1,0,0,2]. The unanchored texture retains the
native empty-peak fault and corresponding independent error. The 128x128 texture
has repeat-identical native output and matches completely with explicit context 2.
The original explicit-context-0 failure remains retained. A completed-worker trace
finds context 2 before Auto; the general file-context producer remains unverified.
Photograph classification is not yet established by these real-GUI captures.

All ten previous native responses, including saved-profile sessions and the
settings-component capture, remain byte-identical with the updated helper.
All three retained-session independent checks and the 17-control constructor
check pass with original reads and child creation denied. Product sources,
the private npm archive and GUI preview are unchanged. Public Auto integration
and complete application parity remain unfinished; nothing has been published.

## Filename-derived context closes the larger-texture mismatch

Completed-worker observation isolates the earlier 128x128 mismatch to its source
context. Explicit zero reproduces the separate native controller's 1529 regions
and final random state 7881. Explicit two reproduces the real GUI's 1530 regions,
all final geometry and random state 311. The empty-edit rebuild hypothesis was
rejected because an empty native edit key skips that operation.

REA 943-951 resolves the producer: encoded-file reading commits its filename to
Shared 0x484b00. Operation 111 verifies fifteen actual MainWindow loads with the
same own PNG bytes under varied suffixes and paths. Canonical resolveImageFilename
now supplies both extension and source context, independently of byte-format
detection. The library exports it with decodeImage, and GUI file opens use it.
The larger texture passes without an override. All 36 successful Auto actions
and 3600 typed parameter records match. The installed package repeats 35/3500
with original/workspace engine reads and child creation denied.

Both retained empty-peak errors still reproduce. The new metadata test passes
all fifteen native cases; its existing decoder test still fails at
adam7-9x7-10. Three source GUI tests, three fresh-extraction GUI tests and the
frontend build pass. The 44-file private package and 73-file GUI ZIP match their
source and fresh installations. No publication occurred. Public Auto, remaining
codecs, manual edits, batch and complete application parity remain unfinished.

## Background removal and retained vector previews

The independent document now owns region visibility and its undo/redo/revert
history. Native point selection tests live smoothed boundaries before searching
neighboring labels. It does not select solely from bitmap labels or intersect
final Bezier curves. Thirty-eight native workflows match 1,371 point queries
and 1,588 complete editor/visibility frames, including one-ULP boundary cases,
65-region histories, source replacement and obsolete region IDs after refitting.
The CLI exposes the same owner through ordered background/settings/process/source
commands. All 77 CLI tests pass; 37 installed recipes produce matching SVG bytes
and one correctly reports unavailable vectors. Installed checks deny original
files, workspace source reads and child creation.

Original public Qt offscreen rendering establishes that the viewer keeps its
last completed vector scene while advanced pixel edits await processing. Current
visibility still affects that scene. Removed transparent regions also receive
red dashed outlines. The GUI retains display geometry separately from current
hit-test/export geometry, commits advanced edits before processing, and exposes
Remove Background through shadcn/ui controls and Lucide tools. Forty native GUI
workflows cover 1,608 frames; source and fresh portable GUI suites each pass
74 tests. A real browser confirms removal, history, retained preview, and export
availability after a completed update. The automation color-fill command failed
to select red on its first attempt; confirmed input/change events and an explicit
settled-state observation establish the subsequent red result. Earlier samples
remain evidence of the harness limitation, not passing color checks.

The full engine/CLI suite passes 403 of 405 tests. The seven 16-bit Adam7 inputs
and the alpha-premultiplied post-edit geometry comparison remain unresolved.
The original advanced commit/undo cache condition can retain matching stage
keys with unavailable boundaries. The implementation preserves stage
availability and reports an explicit hit/export diagnostic, without silently
reusing preview geometry for computation. Auto-Update scheduling, keyboard
behavior, remaining codecs, batch workflows and full application parity remain
unfinished. Nothing has been published. See
[the background contract](background-edit-contract.json) and
`../evidence/background-verification-checkpoint.json`.

## Viewer key-state parity in the modern GUI

ViewerInput now owns the view's tool, Primary/Base/Current layers, B/V event
flags and temporary Pan restoration. React renders owned snapshots and forwards
tool changes through the existing ordered document gesture queue. Keyboard and
pointer producers share their modifier conversion. Six native sequences cover
99 actions, including overlapping layer keys, other keys, Ctrl/Alt/Meta, repeated
Shift, retained restore tools, mode/batch guards and direct focus events.

The source GUI suite passes 81 tests. A real Chromium session verifies 13 steps
with actual CDP key and pointer events: B/V overlap, non-layer key release,
Shift-drag without a visibility edit, Erase restoration and Ctrl-B. This is
bounded viewer evidence, not complete Qt shortcut or focus routing parity.
The engine/CLI source is unchanged from the prior 403/405 checkpoint. Both
existing numerical/decoding failures remain visible. No publication occurred.
