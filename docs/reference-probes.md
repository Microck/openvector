# Native reference recordings

These probes run original instructions during development. They never modify
installation files or enter the published package. Component probes construct
only their engine receivers. The separate GUI reference helper constructs the
real QApplication, MainWindow, widgets and MainModel for workflow capture.
Each Python driver verifies the original executable's SHA-256 before execution.
The Node tests consume recorded numeric fixtures and need neither the executable
nor the probes.

The current recordings use the Windows x86 host CPU. Settings arithmetic uses
x87 control word `0x027f`. Palette assignment uses SSE with MXCSR `0x1f80`.
Recorded values establish behavior for the covered spans on this host, rather
than proving complete application behavior on every machine.

## Saved-profile navigation and reset observations

The separate `profile-replay` helper has three serial paired navigation
workflows in `test/fixtures/profile-navigation-reference.ndjson.gz`. They observe
advanced two-color mode through transparency, palette entry and the subsequent
segmentation timer. Completed segmentation can have region counts and labels
before contour nodes or boundary vectors exist. The reader checks that state
separately from an empty map or palette-prepared labels without regions.

The `profile-lifecycle` build uses `tools/probe-profile-lifecycle-native.c` and
leaves every preceding accepted reference executable unchanged. Its additional
model action invokes original `startOver()` only with a loaded image outside
batch mode. A read-only tail captures the suggested settings name through the
original QLineEdit getter at resolved import `0x6fb7c4`, after validating the
SaveWidget vtable and the MainWindow field established by REA. It never sets
widget text or synthesizes input. Any native modal aborts before an answer.

Three paired reset workflows are retained in
`test/fixtures/profile-reset-reference.ndjson.gz`, with nine complete documents
and 900 typed parameter records. Source loads and full resets recalculate the
first unused exact `Settings N` name. Direct model saves alone leave the field
unchanged. These are model/widget-lifetime observations, not proof of native
modal focus, user text editing or complete GUI conformance.

All original installation, preferences, desktop and owned-namespace guards pass.
Accepted navigation and reset runs execute serially because the Qt namespace is
shared. Three preliminary process executions from accidentally overlapping
captures are excluded and retained in
`evidence/profile-navigation-concurrent-captures-excluded.json`; an initial
reader failure is retained separately. Neither reference data nor original
instructions were rewritten to make those runs pass.

## Build

Run from the project root. These commands use the verified local LLVM tools:

```powershell
$llvm = 'C:\Users\Microck\scoop\apps\llvm\current\bin'
& "$llvm\llvm-dlltool.exe" -m i386 -k -d tools/probe-kernel32.def -l tools/probe-kernel32.lib
& "$llvm\llvm-dlltool.exe" -m i386 -d tools/probe-msvcrt.def -l tools/probe-msvcrt.lib
foreach ($probe in 'settings', 'pixel-map', 'palette', 'refinement', 'peaks') {
    & "$llvm\clang.exe" --target=i686-pc-windows-msvc -fms-extensions -fno-stack-protector -c "tools/probe-$probe-native.c" -o "tools/probe-$probe-native.obj"
    if ($LASTEXITCODE -ne 0) { throw "Could not compile $probe" }
    & "$llvm\lld-link.exe" /machine:x86 /subsystem:console /entry:start /nodefaultlib /base:0x10000000 /fixed "/out:tools/probe-$probe-native.exe" "tools/probe-$probe-native.obj" tools/probe-kernel32.lib tools/probe-msvcrt.lib
    if ($LASTEXITCODE -ne 0) { throw "Could not link $probe" }
}
```

The engine and GUI reference helpers share one build entry point:

```powershell
python tools/build-native-reference.py 'C:\Users\Microck\scoop\apps\llvm\current\bin'
python tools/build-native-reference.py 'C:\Users\Microck\scoop\apps\llvm\current\bin' gui
```

It compiles the original-address reservation and a development C++ caller
boundary, then links with `/safeseh:no`. The helper's NO_SEH flag must be clear
so original C++ frame handlers execute. The first C object owns the unsuffixed
`.text` reservation. Ordinary COFF input/subsection ordering places the boundary
handler and `.text$Z` helper code after that reservation. No generated object or
original instruction rewriting is needed. Installation files stay unchanged.
The x86 `vcruntime140.dll` import supplies the boundary's
handler; this development dependency never enters the npm package.

## Real-widget GUI workflow capture

The separate `tools/probe-gui-native.c` entry constructs the original QApplication
and complete MainWindow, including its real progress bar, MainModel and signal
connections. Original CRT startup initializes the exit registry before running
the original C++ initializer table. The original executable and DLL files remain
unchanged. The accepted engine helper and its recordings remain unchanged.

Run the retained private pilot driver after building the GUI helper:

```powershell
python evidence/record-native-gui-pilot.py
python evidence/record-native-gui-pilot.py file
python evidence/record-native-gui-pilot.py auto
python evidence/create-native-gui-auto-sources.py
python evidence/record-native-gui-pilot.py auto gui-auto-three-colors
python evidence/record-native-gui-pilot.py auto gui-auto-soft-disc
python evidence/record-native-gui-session.py
python evidence/record-native-gui-pilot.py auto gui-auto-opaque-png png
python evidence/record-native-gui-pilot.py auto gui-auto-binary-alpha png
python evidence/record-native-gui-pilot.py auto gui-auto-partial-alpha png
python evidence/record-native-gui-pilot.py auto gui-auto-binary-blended png
python evidence/record-native-gui-session.py transparent
python evidence/record-native-gui-missing-profiles.py
```

The helper has no command-line arguments. Qt 4 reconstructs the Windows process
command line, so recorder paths there would become attempted image filenames.
The driver instead sets `AMBIVALENCE_REFERENCE_SOURCE`,
`AMBIVALENCE_REFERENCE_REQUEST` and `AMBIVALENCE_REFERENCE_OUTPUT`. Operation 105
captures initialized controls and optional file loading. Operation 106 also
invokes the actual `runFullyAutomatic()` slot. Real Qt event processing delivers
the original completion callbacks. Every accepted full response must repeat
byte for byte across two fresh processes. The state-record schema marker stays
105; phase 0/1/2 means initialized, file loaded or automatically processed.

Only recommendation category is initialized in an idle native GUI document.
The other three recommendation words are not yet values. The recorder captures
them only after classification. Failed raw-heap recordings remain retained.

`evidence/verify-native-gui-automatic.mjs` computes each PNG/BMP result using
the canonical document automatic method and observed restored startup controls.
The test helper derives source context from each filename. Reference expectations
are read only after processing; they never supply numerical stage inputs. Permissions deny original
executable reads and child creation during the comparison. Three successful
pilots cover two artwork categories and two palette modes. They check selected
controls/color count, candidate caches, all 100 registered parameters, pixels,
complete contours, smoothing metadata, fitted geometry and assembled chains.

Operation 107 captures a retained real GUI session: four file loads and eight
Auto actions, including a return to the first source. The complete response
repeats byte for byte in two fresh processes. The independent comparator uses
one engine session and computes every state before reading reference
expectations. Loaded controls, palette clearing, original pixels, reset
recommendation category and retained PRNG agree; all eight processed states
also match, including 800 registered parameters and complete geometry. This
covers the observed preferences-restored lifecycle and retained-control reset.
Clean-install defaults and preference persistence remain separate requirements. Shared readers in
`tools/engine-reference-codec.py` parse both fresh and retained GUI states.
See `../evidence/native-gui-session-verification-checkpoint.json`.

Four RGBA PNG pilots add opaque, binary-alpha and partial-alpha coverage. A
separate retained PNG session adds five loads and ten Auto actions. All fresh
responses repeat exactly; independent composition matches complete states and
geometry, including 1400 parameter records. The binary-alpha blended-artwork
case exercises native background command 3, which sets mode 1 and opaque white.
Other covered cases use command 0, mode 0 and transparent white. Source alpha
classification reads the prepared original, including across repeated actions.
See `../evidence/native-gui-transparency-verification-checkpoint.json`.

REA 918 establishes that MainModel startup reads QSettings profiles and then
sets its +0x2dd lifecycle flag. It is not a user reset-preference control. The
captured starting values match the current restored default profile. A
read-only whitelist of twenty non-secret controls, including nine exact startup
field comparisons, is retained in `../evidence/native-gui-startup-profile-provenance.json`.
The whitelist inspection wrote no registry value. The component and isolated
MainWindow captures below establish missing-profile defaults separately from
the existing saved-profile captures.

Operation 108 constructs three real native settings QObjects, using 0x444510.
It captures the raw constructor and reads absent default/batch profiles through
0x442bf0. The member call uses hidden QString return storage followed by index
and group QString values; the original callee destroys those two value inputs.
Both complete 628-byte responses repeat exactly. Missing keys remain absent,
and the twenty whitelisted fields in each saved default/batch profile agree
before and after. No profile key is created for this capture.

The independent ImageDocument constructor matches all 17 corresponding native
constructor controls. The missing default profile changes background from
transparent to opaque black, palette-color count from 2 to 3, and recommended
background mode from -1 to 0. The missing batch profile additionally uses Auto
sentinels 3 for category, quality and palette mode, with background mode 1.
These component values do not establish final batch output. MainModel later
forces its selected palette-color count to zero at 0x43027c.
See `../evidence/native-gui-profile-defaults-verification-checkpoint.json`.

Operation 109 selects a previously absent, capture-owned application preferences
namespace through Qt's public application-name setter before the real MainWindow
constructor. It records four own PNG/BMP loads and eight Auto actions, with no
settings injection or instruction/license-check patch. Both complete 135041-byte
responses repeat exactly. Independent processing from declared clean-profile
inputs matches 800 typed parameter records, selected palette/background metadata,
all caches, pixels and complete final geometry. The capture checks both saved
numeric profiles and removes only its owned preferences namespace after each
process. Final batch processing and general source-shadow defaults remain open.

```powershell
python evidence/record-native-gui-session.py clean
```

The own 31x27 noisy source yields [1,0,2,12], proving that flexible palette mode 2
skips palette search before segmentation. Independent processing matches its
entire final state and geometry. A 48x48 anchored texture also matches, with
[1,0,0,2]. An unanchored texture retains the native empty-peak fault and matching
independent error. A larger texture fails with explicit context 0 and matches all final fields with
explicit context 2. The original failure is retained. Its GUI context is 2 before
Auto, but a general encoded-file default has not been established. Photograph classification is not established.

The procedural gradient faults in the native classifier on an empty peak array.
The independent classifier reports the corresponding empty-histogram error.
Both failures remain visible; full GUI error handling is still unverified.
See `../evidence/native-gui-workflow-verification-checkpoint.json`. These private
tools and native binaries do not enter either independent distribution.

Operation 110 joins each actual QThread before dispatching its original queued
GUI completion. It stores four completed-worker observations and all 100 typed
parameters per worker, then appends the trace after the unchanged full response.
Two fresh traces repeat exactly. The 1947112-byte existing GUI response remains
byte-identical. This observation changes event pacing and does not establish
in-flight scheduling or cancellation behavior.

```powershell
python evidence/record-native-gui-worker-trace.py
```

The trace observes raw source-context word 2 before Auto and after each worker.
With explicit input 2, both the independent composition and separate native
controller match the real GUI's complete final state and random state 311.
Explicit input 0 instead yields 1529 regions and final PRNG 7881; that failure
remains retained. The comparison now requires the context as an argument.
Neither the observation nor the conditioned comparison establishes a general
file default. The empty-edit rebuild hypothesis was rejected: its native cache
key is empty, so the operation is skipped. See `native-gui-source-context.md`.

Operation 111 captures the filename metadata produced by fifteen actual
MainWindow loads. Every input contains the same own procedural PNG, including
files with misleading JPEG/GIF/TIFF suffixes. Both complete responses repeat;
all decoded pixels match the independent decoder. This proves filename context,
not those additional codecs. Long native strings use their actual heap storage.

```powershell
python evidence/record-native-gui-source-contexts.py
```

The setter chain 0x46a9a0 -> 0x46e890 -> 0x484b00 resolves the earlier source
context gap. The canonical library resolver and GUI input now reproduce it.
All 36 successful file Auto comparisons pass with filename-derived context,
including the larger texture. The original zero-context failure is retained.
See `../evidence/source-context-verification-checkpoint.json`.

## Record and verify

```powershell
python tools/record-document-cancellation.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/record-settings-native.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/generate-settings.py
python tools/probe-pixel-map.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/probe-palette.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/probe-refinement.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/probe-peaks.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/probe-peaks.py 'D:\VectorMagic\Vector Magic\vmde.exe' generation
python tools/probe-peaks.py 'D:\VectorMagic\Vector Magic\vmde.exe' measurement
python tools/probe-peaks.py 'D:\VectorMagic\Vector Magic\vmde.exe' weighted-generation
python tools/probe-peaks.py 'D:\VectorMagic\Vector Magic\vmde.exe' weighted-refinement
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' loader
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' generation
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' weighted-generation
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' score
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' pipeline
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' merge-cost
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' initial-regions
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' region-pass
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' unweighted-segmentation
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' diagonal-contacts
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' unweighted-diagonal-segmentation
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' blend-sample
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' weighted-score
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' subpixel-statistics
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' subpixel-merge-cost
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' subpixel-merge
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' subpixel-pass
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' subpixel-rebuild
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' subpixel-optimization
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' presegmentation
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' anti-alias-segmentation
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' subpixel-color-selection
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' clustered-anti-alias-segmentation
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' supplied-palette-segmentation
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' subpixel-palette-projection
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' contour-extraction
python tools/probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' curvature-prior
npm test
```

Settings captures contain 648 combinations across the 12-position sliders and
related switches. The generator writes typed profile facts and finite slider
tables to `src/parameter-data.json`. The original numeric instruction spans end
before palette allocation and external calls. Their absolute constant operands
point into a private copy; a return instruction marks the exclusive end.

Pixel-map captures contain 5,574 edits. All scanline decisions and writes execute
the original integer instructions. The harness relocates direction operands and
redirects deque insertion/destruction to a bounded harness-owned deque. It does
not replace the flood-fill algorithm.

Palette captures contain 1,193 cases with 12,032 pixel assignments. Weight
collection is disabled, so the recorded assignment path makes no calls. Its one
executed constant operand is relocated; the SSE arithmetic and comparisons are
unchanged. Cases include random palettes, repeated colors, and values adjacent
to every normalized byte value in each channel.

Refinement captures contain 89 ordered calls in one original PRNG session. The
original assignment, means, stopping decisions, empty-cluster replacement, and
random instructions run on the host. The harness supplies starting palettes and
preallocated vector operations. Cases exercise every observed iteration count
from 2 through 10 and verify state carry-over between calls.

Peak captures contain 44 scans over 8-bin and 16-bin histograms with both
neighborhood modes. Original list sorting and merging run along with the peak
detector. This preserves the original unusual equal-weight channel comparison.
The harness supplies bounded allocation, vector capacity, constructor iterators,
and cookie-check scaffolding. Decoded operand relocations are recorded in
`evidence/peak-probe-relocations.json`.

Generation captures contain 74 complete unweighted fixed-count color-stage runs,
including transparent, repeated, random, single-pixel, and empty inputs. Original
histogram construction, peak detection/sort, starting colors, refinement, and
final quantization run unchanged. Candidate scoring and retained-result copying
are omitted after checking that they do not modify these palette colors or pixel
assignments. Automatic selection is outside this reference's scope. Its decoded
relocations are in `evidence/generation-probe-relocations.json`.

Measurement captures contain 471 cases with dimensions of at least 2 by 2.
Original reflected-neighbor sampling, mixed float32/double arithmetic, square
roots, clamping, and byte truncation run on the host. The harness supplies the
byte-grid allocation. One-dimensional application handling remains unresolved;
the reference does not substitute clamped edge pixels for native reflection.

Weighted generation and refinement each have 80 ordered cases. The original
weight-map constructor supplies strength 50 and minimum 0.01. Original weighted
histogram accumulation, float32 means, raw-contrast membership collection,
convergence, empty-cluster replacement, and PRNG state all run on the host.
Generation includes native peak initialization and final quantization;
refinement starts from supplied palettes and records unquantized colors.
Candidate scoring and retained-result projection remain outside these recordings.

The engine probe reserves the original address range inside a private x86 helper
before Windows allocates its heap. It copies PE sections to their expected
addresses, resolves all 1,814 imports, initializes the six parameter registries,
and calls the original core constructor. It patches no original instructions and
does not enter application startup. The driver builds this helper automatically
and checks the original executable and installation DLL hashes before running.
Only this development oracle loads the installed reference DLLs. The package
and its tests have no dependency on them.

Complete fixed-count palette runs include scoring, anti-alias blending, real
allocation, and retained-candidate copying. Their colors, indices, membership
weights, and PRNG states agree with the earlier bounded captures for the ordered
prefixes of 69 unweighted and 63 weighted cases. These prefixes stop before the
first count above 24, the constructor's allocated retained-candidate range;
skipping a call would change subsequent random state. This does not establish
GUI limits or end-to-end vectorization parity.

The score recording contains 1,235 calls to the unmodified unweighted candidate
scorer at `0x48d180`. It covers supplied nearest and non-nearest indices, 1 to 256
palette entries, empty inputs, 4,096-pixel accumulation, normalized-byte boundary
values, and several explicit color penalties. The independent scorer reproduces
all three float32 outputs exactly on these cases. Weighted scoring remains
separate because it estimates spatial color blends near boundaries.

Blend-sample mode executes the original one-, two-, and multi-color solver for
1,080 samples with one through nine ordered local colors. It records float32
output colors, double weights, and double self weights. Multi-color captures
also retain the native factor, projection, constraints, and solved matrices to
locate numerical drift. The independent solver matches all recorded outputs
exactly, including tiny negative weights allowed by the native tolerance.
Weighted-score mode captures 440 original calls using explicit assignments,
empty and narrow maps, and palettes up to 256 entries. Original boundary masks,
neighborhood selection, blend fitting, and float32 error accumulation run
unchanged. No image decoder or anti-alias measurement kernel is substituted.
See `docs/blending-contract.json` for the precision and stage boundaries.

Subpixel-statistics mode first establishes dense base-region labels and native
record capacities, as required by the original pipeline. It then runs original
subpixel initialization and, for the wide cases, the original 24-neighbor rebuild.
The 540 cases span nine dimensions, five pixel patterns, six label patterns,
both neighborhoods, and explicit self-weight controls. The recording contains
labels, cached pixel costs, all/interior moments, median or mean region colors,
sorted boundary sets, and double interior/total errors. The independent rebuild
matches every recorded field. This is a statistics/energy reference; it does
not execute or establish the subsequent anti-alias merge/move schedule.

Subpixel-merge-cost mode records 10,708 directed proposals on 540
prepared states. It captures the cached original error, target-color interior
error, recomputed boundary error, and threshold-bounded proposal cost. The
recording exercises both lower-bound rejection and full recomputation.
Subpixel-merge mode records 838 applied merges across 340 images, including
sequences of up to 8 merges. Every incremental statistic, retained color,
boundary set, cached pixel cost, and subsequent proposal cost matches the
independent model. The state capture projects union roots without rebuilding
statistics. Neither mode invokes the complete anti-alias optimization schedule.

Subpixel-pass mode runs five ordered passes on each of 540 reconstructed
states. All 2,700 pass return counts and full numerical snapshots match,
including 18,350 accepted merges. Controls cover both source-count modes,
independent source/target size limits, increasing thresholds, and repeated
passes that must reconsider examined pairs. The original clears pair flags
at each pass end; the independent implementation uses a per-pass packed-key
set with the same lookup behavior. The recording includes 95,380 region
records and 385,800 cached pixel values. The complete schedule and later
pixel-move cleanup remain separate stages. The bulk reference process has
a bounded 120-second timeout; its first 30-second limit expired after 2,536
completed passes, and that partial recording did not supply test expectations.

Subpixel-rebuild mode records 540 states after explicit merges, runs the
original thin-region cleanup, then alternates the neighborhood twice. All
1,080 rebuilt snapshots match the independent model, including retained
interior-cache entries. Cleanup changes none of the 540 input states, which
contain 17,775 active zero-interior regions. Assembly rejects each such root
when its boundary set includes a foreign pixel; a valid zero-interior region
necessarily has one. The independent full schedule omits this ineffective step.

Subpixel-optimization mode executes original 0x4a9ac0 from explicit base-region
labels. All 432 final states match, including both optional-wide modes,
constructor-bounded iteration loops, every rebuild, and diagonal repair.
This stage does not invoke preceding presegmentation, subsequent shadow/color
clustering or palette transfer, image decoding, smoothing, fitting, or export.
The shared diagonal repair still matches its 994 original maps and all 328
complete unweighted segmentation runs.

Presegmentation mode runs original 0x4ab730(0) under all ten profiles and
selected advanced controls. Its 632 snapshots match labels, counts, float32
moments and color identifiers before palette extraction. The original always
uses axis passes here, regardless of the later diagonal-extraction flag.
Anti-alias-segmentation mode runs the original automatic branch with fresh
shadow/clustering defaults off. All 240 runs match complete labels, counts,
quantized palettes, and color identifiers, including 168 basic runs and 72
advanced runs with clustering disabled. The segmentation CLI supports all
nine basic category/quality combinations. Advanced clustering is verified separately below.

Pipeline mode now records the palette immediately after segmentation. Its
driver writes the minimal pipeline-segmentation fixture from the complete
original recording and updates its provenance digest on every run. Independent
segmentation matches all 15 known-settings stages, including prepared pixels
from the original filter and the original palette-stage integration context.
This does not establish independent smoothing, fitting or exporter parity.

Subpixel-color-selection mode records 548 original before/after states and
7,832 unrounded bounded boundary costs. Its tiled stress cases contain up to
122 eligible candidate regions, exceeding the original cap of 100. Candidate
order follows descending interior count, then descending region identifier.
The independent controller reproduces all fields, including 48 actual color
replacements, while retaining labels, moments, boundaries and cached pixels.
Clustering-enabled automatic segmentation matches 240 complete native runs;
eight differ from the corresponding unclustered outputs. Fresh shadow flags
remain off. The CLI exposes this option at the tested advanced complexities.

Supplied-palette mode records 782 complete native results: 302 plain and 480
anti-aliased. Colors include duplicate and unused entries, non-byte float32
channels and palettes of 1, 2, 3, 8, 24, 25 and 256 colors. Both clustering
settings produce the same supplied-palette behavior. A further 482 plain inputs
have fewer initially allocated statistic records than palette entries. The
native marker at 0x489300 writes one record per color; the first unrestricted
experiment caused an access violation at case 5. The retained native preflight
records those unsafe inputs separately, before dispatch, without altering any
original instructions. They do not count as successful parity cases.

Fixed-palette projection records 1,644 complete states with thresholds 0, 3
and 20. Repeated projection retains assigned colors. Twelve adversarial cases
distinguish double accumulation from float32 assignment, and twelve test
duplicate ties below a double ulp. Recorded x87 control word 0x027f confirms
53-bit precision. Labels, moments, boundaries and cached costs remain unchanged.

The segmentation recordings cover four distinct boundaries. Merge-cost mode
supplies two original statistic records and captures 1,042 float32 color costs.
Initial-regions mode captures 300 recursive tile maps, including clipped tiles
and narrow images. Region-pass mode captures 945 merge/move or minimum-size
passes, including union-root projection, empty statistic slots, channel sums,
and color identifiers. Unweighted-segmentation mode runs the complete original
branch for 160 prepared images under profiles 0, 7, 8, and 9 with diagonal
extraction disabled. It records final region labels, counts, palette colors,
and region color identifiers. The independent implementation matches each
recorded value. These recordings execute unpatched original instructions;
the harness supplies explicit inputs and settings. The anti-alias recordings
above cover the separate branch; nonzero size penalties remain unfinished.
See `docs/segmentation-contract.json` for arithmetic and ordering requirements.

Diagonal-contacts mode runs an original eight-neighbor pass, records its map,
then runs original diagonal-contact repair and captures the resulting map and
unchanged statistics. Its 994 cases include all binary topologies at the selected
dimensions through 3 by 3, plus larger random three-color images and starting
labels. The independent repair matches every recorded map, including real
replacements and retained ties. Unweighted-diagonal-segmentation mode runs the
complete original branch under profiles 1, 2, and 3 for 168 cases. The capture
includes final labels, counts, normalized palettes, and region color identifiers.
All match the standalone implementation. These are prepared-pixel engine tests;
they do not establish image decoding, preprocessing, or full conversion parity.

The pipeline recording runs the original input processing, fixed-count palette,
segmentation, contour extraction, smoothing, fitting, output assembly, and six
exporters for 15 known-settings images. It records all 100 numeric parameters,
prepared BGRA pixels, segment maps, referenced cubic coordinates, region parents,
and per-stage random state. The original input-cache initialization and
per-image filter invalidation are included. Read `engine-reference-contract.json`
for the exact scope and `evidence/engine-pipeline-repeatability.json` for repeated
run results. The corpus has 90 files under `evidence/engine-reference-exports`.

Geometry captures follow region edges to referenced cubics, excluding unused
reserved slots. Native edge kind 0 is a line; kind 1 draws a cubic forward; kind
2 draws it backward. The original edge renderer at `0x4741b0` confirms this.
Anti-alias smoothing consumes the same PRNG as palette generation. The capture
preserves that coupling between successive images.

The 15 numeric pipeline results repeat exactly. Of the 90 exports, 75 repeat
byte-for-byte. The 15 EPS files differ only in creation dates, which remain
observable contract fields. SVG XML is parsed; other formats receive basic
signature/trailer checks, not full semantic validation. These reference outputs
are not a claim that the independent engine can yet generate matching geometry.

The earlier `tools/probe-settings.py` Unicorn experiment is retained to reproduce
the emulator/host discrepancy. Its outputs are not golden fixtures. The 888
observed field differences are in `evidence/settings-emulation-comparison.json`.

The canonical unit suites own component behavior. CLI tests own process input,
argument validation, and JSON serialization. The native harnesses and captured
fixtures are retained as durable differential evidence, separate from runtime
source and package contents.

Contour-extraction mode imports completed segmentation labels and palette color
identifiers, initializes original direction offsets, rebuilds original statistics,
and verifies that the import preserves every segmentation field. It then invokes
0x499eb0 before any smoothing. Record initialized numeric fields only: the first
byte of each boundary record is a flag; its next three padding bytes contain
allocation residue and are excluded. All 2,584 results match independent node
coordinates, kinds/flags, ordered contours, boundary metadata, neighbors and
parents. The corpus includes 1,312 exhaustive binary topology cases, 112 reflected
long staircases and four nested-ring cases. Fresh native processes reproduce the
complete binary recording. The pipeline probe also captures all 15 contour stages
before smoothing, and the independent segmentation-plus-contour path matches each.

Curvature-prior mode supplies three double coordinate pairs, two positive original
sample counts and a double length weight to the original type-3 curvature energy
and derivative functions. It also invokes the original quadratic fallback used
for small regions and inversion penalties. There are 5,010 finite curvature cases,
six collapsed-edge NaN cases and 5,016 quadratic cases. Native and independent
energies and both endpoint derivative vectors match exactly. Assembly corrected
the decompiler's derivative regrouping; no tolerance replaces equality. The
pipeline probe now captures complete original post-smoothing geometry and shared
PRNG states for all 15 images before fitting. These target states do not establish
independent complete-smoothing parity.


## Prepared smoothing objective and controllers

```powershell
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' smoothing-objective
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' smoothing-line-search
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' smoothing-optimizer
```

Mode 24 imports completed segmentation, checks its fields, invokes original
contour extraction and prepares the original coordinate/gradient buffers. It
then supplies explicit positions, float32 reference coordinates, node flags,
byte penalty counts and boundary sample records. Original 0x4941e0/0x495b00
provide energies and border-constrained gradients for 5,598 states: 459 topology
frames across three phases, measurements 0/2 and raw/jittered variants, plus
90 states drawn from complete original post-smoothing geometry. Prior codes
0/3, small-region pressure, flag-0x10 inversion priors, reference dead zones and
byte-count-scaled inversion measurements are covered. This boundary forces the
shared anti-alias flag off; image-based measurement and coverage refresh are
not represented by these geometric measurements.

Mode 25 adds an explicit direction and epsilon, copies the selected phase CG
configuration, and invokes original 0x4ad010. The 3,312 cases cover descending,
ascending, zero and scaled directions, bounded/unbounded maximum displacement,
multiple line iterations and both next-epsilon modes. Capture accepted positions,
energy, returned delta, accumulated step, next epsilon and line count. The original
0x493640 callback resets image borders and clamps the prepared near-border node
cache after every move, including restored/zero moves. That cache is captured
as node identifiers and stays fixed throughout the search.

Mode 26 invokes complete original 0x4adf00 with optimizer and line-search type 0
on the same prepared geometric model. Its 588 cases cover all phases, varied
minimum/maximum counts, restart intervals and stopping tolerances. Capture the
final positions and energy, stored gradient, direction, baseline, epsilon,
accepted step, beta, iteration field, restart count, line count and knockout
countdown. Original 0x493610 lacks a Ghidra function definition; retained objdump
assembly establishes that its anti-alias-only restart side effect is inactive
at this boundary. The preevaluation callback 0x6909f0 returns immediately.

All three independent components match every captured numerical field exactly.
Each native recording repeats byte-for-byte in a fresh process. This is prepared
geometric-controller parity, not complete smoothing parity. Initial anti-alias
preparation/jitter, shared PRNG integration, image-based measurement, coverage
refresh, other optimizer modes, corner puncturing and multi-phase dispatch remain
unfinished. See `docs/smoothing-contract.json` for their current recovery state.


```powershell
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' smoothing-cached-optimizer
```

Mode 27 uses the same prepared geometric model and complete Fletcher-Reeves
entry with line-search type 5. Its 441 cases compare every controller field
captured by mode 26. These cases exercise all phases, varied seeing/blind cycle
lengths and blend fractions, and bounded large accepted-step estimates. Native
and independent results match exactly. Counter comparisons prove rejected blind
moves in 187 runs and cycle wraps in 321 runs. Line type 5 alternates original
quadratic searches and estimated moves, restores coordinates without a callback
when an estimate raises the energy, and immediately retries a seeing move.
Both line-search types used by the recovered GUI profiles are now implemented
for the prepared geometric model. Image-based anti-alias measurement and coverage
refresh remain outside this boundary.


## Stateful image-based contour measurement

```powershell
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' smoothing-image-measurement
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' smoothing-image-sequence
```

Mode 28 imports completed segmentation and shared contour geometry, initializes
original measurement storage with 0x4880a0, and supplies explicit double node
coordinates. It calls original 0x488710 and 0x487fe0 directly, before geometric
priors and final border-gradient constraints. Capture energy, float32 cached
BGRA colors, byte invalid-coverage flags, all initialized clipped records and
raw double node gradients. Gradient evaluation leaves the captured colors
unchanged. The 2,802 cases cover 459 raw/jittered topology frames across all three phases,
15 original post-smoothing pipeline images in each phase, exact quarter-pixel displacement and near-horizontal
directions at exponents -70/-90.

Mode 29 retains the original model between explicit coordinate evaluations.
Its 1,425 cases produce 6,981 raw, jittered, repeated, translated and restored
states. Exactly 652 sequences have different energy after restoring their first
coordinates because the original pixel-color cache persists. This observable
state must not be replaced by reinitialization on every evaluation.

Independent results match all captured fields exactly, including 3,272,031
initialized clipped records across both corpora. Native recordings repeat
byte-for-byte. Assembly establishes float32 channel normalization/residuals and
double energy/gradient accumulation, independently rounded coordinate/perimeter
stores, truncating grid casts, and extended FPATAN register retention through
subtraction before storing the contour turn. Decompiler casts alone do not
preserve these boundaries. The standalone measurement model does not yet
integrate full smoothing priors, optimizer phases, anti-alias preparation,
coverage refresh or corner puncturing.


## Prepared image smoothing integration

```powershell
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' smoothing-image-objective
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' smoothing-image-line-search
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' smoothing-image-optimizer
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' smoothing-image-objective-sequence
```

Mode 30 captures original combined 0x4941e0/0x495b00 energies and constrained
gradients with measurement code 1, all geometric priors/pressure/anchor terms,
and explicit prepared image state. Its 2,799 cases span all phases and raw,
jittered and original final-smoothed geometry. Mode 32 captures 3,312 original
explicit quadratic line searches on that stateful objective. Mode 31 captures
1,029 complete original Fletcher-Reeves runs with profile line-search types 0/5.
Mode 33 retains one original image model across 6,981 combined-objective
energy/gradient evaluations in 1,425 coordinate sequences.

The independent kernel shares the verified image traversal and derivative code
between the standalone model and smoothing. It validates opaque model binding
once at the public boundary, updates image colors/records only during energy
calls, and adds each image derivative directly into the existing prior gradient.
Gradient-only refreshes do not invoke another image-energy traversal. These
instruction-order details are observable in exact comparisons.

The broader three-phase corpus exposed one folded-contour perimeter error.
Original 0x4865b0 subtracts stored float32 turns and adds its double 2*pi constant
without an intervening float32 round. Retaining that double wrap fixes source
objective 2217 and an affected line-search trajectory; no tolerance was changed.
All captured results now match exactly and all six image-related recordings
repeat byte-for-byte. These prepared probes disable the shared anti-alias repair
callback flag. Initial preadjustment/jitter, crossing/repair callbacks, corner
puncturing and full phase dispatch remain separate unfinished integration work.

## Prepared anti-alias repair and complete prepared controllers

```powershell
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' smoothing-repair
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' smoothing-repair-optimizer
```

Mode 34 retains original nearby-edge associations built before explicit moved
coordinates are supplied. It records 14,106 states across 1,425 sequences of
whole-model repair, individual-region repair, byte-penalty decay and image-error
region selection/repair. All changed counts, node flags/penalties, defined probe
directions and winding seeds match exactly. All 2,850 original error-region sets
match, including 274 nonempty sets. The first action repairs every region before
capturing its probe slots, so those recorded fields are initialized.

Mode 35 retains the same original preparation associations and enables the
shared anti-alias callback flag for 1,029 complete original controllers. It
captures all standard controller outputs and final node flags/penalties. Unvisited
region probe slots can contain uninitialized storage and are excluded from this
controller recording; every region repair seeds its probe before using it.
All phases and both line-search types 0/5 are covered. Native interval 10 repairs
all regions after penalty decay; other iterations repair sorted image-error
regions. Changes restart from the stored gradient and refresh energy/baseline
before stopping checks. Both new recordings repeat byte-for-byte.

The independent controller owns supplied associations, node flags and byte
penalties. Independent association construction, blend preadjustment, shared
PRNG jitter, corner puncturing and full phase dispatch are still required before
complete smoothing parity can be claimed.

## Independent fixed contour-neighborhood preparation

```powershell
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' contour-neighborhoods
```

Mode 36 imports explicit double coordinates, byte node flags and shared region
contours before calling original 0x4822b0. It records the fixed [region,sample]
association lists and final byte flags for 2,814 cases, including 12 targeted
collapsed/tied/threshold/anisotropy/reflection/pre-marked variants. Every ordered
association and flag matches independently: 64,518 nodes and 1,045,003 entries.
The fresh recording repeats byte-for-byte. The native helper still reproduces
all earlier repair and controller recordings without numeric or byte changes.

The independent builder follows recovered 0x482190/0x5b9130 tree construction,
0x5bb2b0 sliding-midpoint splitting, 0x5baed0 partitioning, 0x5b9470/0x5b95e0
near/far traversal and 0x5b9310 stable capped insertion. The index includes every
contour sample, including shared-node duplicates, but requests only
min(50,nodeCount) neighbors. Native epsilon is exactly 1e-12, and candidates
must have squared distance strictly below 25 and a different start-node ID.
Queries are indexed nodes, so their initial root-box distance is zero.

Original prepared neighborhoods for modes 34/35 can now be recreated from
independent extractContours followed by prepareContourNeighborhoods before
supplying moved coordinates. All 2,454 preparation lists match the retained
native lists, and the repair/controller regressions use the independent lists.
Initial blend preadjustment, shared PRNG jitter, corner puncturing and complete
phase dispatch still remain before complete smoothing parity.

## Contour preadjustment and shared session initialization

```powershell
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' smoothing-preadjustment
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' smoothing-initialization
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' smoothing-initialization-mixed
```

Mode 38 calls the original 0x495720 on interior nodes after original 0x4a1850
palette-mode setup. Mode 37 calls complete original 0x495a30. Each captures
2,604 cases: 1,447 anti-aliased and 1,157 plain. All coordinates and retained
random states match exactly. The corpus includes 55,788 anti-alias interior-node
visits, 111,576 uniform draws, plain no-ops and zero-amplitude draw consumption.
Preadjustment changes 37,310 node states; initialization changes 55,386.

Mode 39 interleaves real original eight-color palette generation for a black
pixel with contour initialization. All 110 runs match palette outputs, coordinates
and shared random states. The independent CoreSession uses that same stream for
palette and contour operations. Palette storage rounds to float32; contour
jitter consumes raw double uniform values. It does not use Gaussian noise.

The original diagonal basis is 1/Math.sqrt(2), whose last bit differs from
Math.SQRT1_2. Original float32 reference coordinates truncate before subtracting
one. Repeated colors in the four-pixel quad contribute repeatedly, while each
sample's first/self palette entry is skipped. Exact operation order is retained.
All three recordings repeat byte-for-byte. Reference-grid preparation, corner
puncturing and complete supported-profile smoothing are recorded separately below.

## Raw contour geometry and fixed near-border preparation

```powershell
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' contour-preparation
```

Mode 40 calls original 0x496740 with perturbation disabled before supplying
explicit moved coordinates. It captures initialized geometry, float32 references,
penalty bytes, compacted border groups, ordered near-border identifiers and fixed
anti-alias neighborhoods. It then calls original 0x482490 on the explicit
coordinates to isolate near-border selection. No process pointers are serialized.

All 2,700 preparations and explicit near-border selections match independently.
The corpus contains 1,447 anti-aliased and 1,253 plain cases, 128,160 node states,
21,042 prepared near-border entries and 2,620,080 fixed neighborhood associations.
Its 96 direct threshold probes cover both axes, lower/upper edges, all four
unrolled-loop remainders, 48 adjacent-double inputs and 48 float32-neighbor inputs.
Original PRNG state stays at one throughout this preparation experiment.

Executable constants at 0x8de080 (float32) and 0x8de088 (float64) both contain
1.100000023841858. Assembly performs double comparisons on both axes. Apparent
bottom-edge float casts in decompiled text are not actual instructions. Keep
strict comparisons and compute these lists before jitter or optimization moves.
References preserve original float32 grid coordinates and penalties start at zero.
The recording repeats byte-for-byte; the test fixture matches its recording bytes.
Independent preparation also composes with all 2,604 retained-session native
initializations. Corner puncturing and full phase dispatch are recorded separately below.

## Corner decisions and complete puncturing

```powershell
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' corner-decision
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' contour-puncturing
```

Mode 42 calls the original fixed classifier 0x4adfb0 with explicit 21-double
feature vectors. Its return occupies AL only; the harness zero-extends that byte
rather than treating the unchanged upper EAX bits as a result. All 3,504 decisions
match independently, including 1,248 adjacent-double threshold probes and
576 vectors with NaN or infinity in a used field. Nonfinite fixture values are
stored as strings and converted to doubles only for native input/testing.
All 13 decision constants match their executable double bytes exactly.

Mode 41 calls complete original 0x494a90 after loading explicit geometry,
including caller node kinds and boundary flags. It captures every resulting
coordinate, kind, flag, region and topology field. All 3,680 cases match,
including 1,029 repaired optimizer results, 15 fully smoothed original pipeline
states, collapsed edges, transformed coordinates, existing corner markers and
finite coordinates that overflow intermediate differences. These cases cover
189,170 eligible windows and 29,211 skipped short-region states. Puncturing
changes 25,461 node kinds, sets 66,034 boundary corner bits and sets 118,378 turn
maximum bits without clearing existing markers. Coordinates and topology remain
unchanged. Original PRNG state stays at one throughout both experiments.

The independent kernel evaluates only the six feature fields used by the fixed
original classifier. Unused length/median fields are not exposed as a public API.
It preserves cyclic windows, zero-length directions, exact arithmetic, peak ties
and x87 unordered comparison behavior. Both recordings repeat byte-for-byte and
both fixture files match their recording bytes. Complete smoothing is recorded
separately below.

## Complete smoothing and retained fatal outcomes

Build the current helper through the ordinary loader driver, then run the
supervised session recorder:

```powershell
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' loader
python .\tools\record-full-smoothing-sessions.py 'D:\VectorMagic\Vector Magic\vmde.exe'
```

Mode 43 calls complete original 0x497c30 once on raw geometry. Unlike the
prepared-component probes, it does not call 0x496740 separately first. The wire
header contains three little-endian uint32 values: operation, input count and
initial PRNG state. The canonical session recorder always starts each original
process at state one and retains that stream between inputs. Explicit node kinds
are serialized for modes 41 and 43. Shared serialization lives in
`tools/engine-reference-codec.py`; both probe drivers use the same definitions.

The corpus contains 2,584 raw geometry cases, 27 cases spanning all nine basic
profiles, 96 phase-mask/corner-schedule cases and six explicit seeing/blind
line-search cases. One original process completes 775 inputs and faults on
case 775. The recorder retains that failure and starts a new original process
for the next input. The second process completes all remaining 1,937 inputs.
No failing case is omitted, retried for a vector, or assigned invented output.
The fixture contains all 2,713 outcomes and explicit process/session boundaries.

The original odd-winding diagnostic path requires registry 0xa69048, constructed
by original 0x484fd0 in startup entry 0x6f8e00. The helper invokes that constructor
without patching instructions or suppressing diagnostics. The later fatal
outcome at 0x486b6d is distinct: eleven contiguous fragment groups at pixel (2,2)
overlap nominal identifier/weight storage and overwrite the count with
1315640065. The captured failure state includes geometry, clipped records and
scratch bytes. It reproduces with stage-start state 4789; the same image at
state one completes, showing why input-only special cases would be wrong.

The independent model uses one packed little-endian numeric record: identifiers
at relative 0, double weights at 0x28, signed count at 0x78 and signed background
at 0x7c. It preserves write/read order, stale scratch bytes and the original
one-past-count background subtraction. A fragment cap is invalid because some
native overflowing cases complete. Invalid numeric-record/region access raises
a JavaScript exception in the independent implementation, rather than a process
access violation. Failure outcome parity does not claim identical crash/recovery
semantics. `CoreSession` restores its stream when the library operation throws.

All 2,712 successful final geometries, penalty arrays and retained random states
match exactly, as does the one fatal conversion outcome. All 15 established
full-pipeline smoothing targets match at their recorded stage-start states.
The full outcome corpus repeats identically. Failed-process diagnostic context
contains addresses, so whole failed binary files are not claimed byte-identical.
See `evidence/smoothing-full-sessions-repeatability.json`. Tests consume only the
numeric fixtures; installed library and CLI checks need no reference executable.

## Fitting chord-length sample preparation

Build the current helper with the loader command before recording:

```powershell
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' loader
python .\tools\record-fitting-samples.py 'D:\VectorMagic\Vector Magic\vmde.exe'
```

Mode 44 directly calls original 0x49c7b0 on the existing fitter at core+0x1f08.
Inputs contain shared finite double node positions and byte node kinds, ordered
cyclic contour identifiers, start/end indices and an explicit marking switch.
It supplies the interval endpoints from the same contour and captures returned
interior sample coordinates/parameters, all final node kinds and PRNG state.
Temporary vectors use original allocation and matching original delete[].
This experiment does not invoke keeper selection, fitting, image measurement
or preprocessing and does not imply a public prepared-fitting API.

The 2,784 cases contain 7,408 interior samples. They cover every retained
smoothed pipeline image, eleven completed full-smoothing geometries, explicit
single-node and repeated-node contours, collapsed spans, both marking modes,
empty/adjacent intervals, full cycles and wrapped ends. Scale probes include
2^-70 and 2^-90. All calls leave random state at one. The complete native binary
and fixture repeat byte-for-byte; `evidence/fitting-samples-repeatability.json`
records the hashes. At that initial capture checkpoint, independent sample preparation and complete
fitting were pending. Their completed comparisons are described below.

## Interval costs, keeper selection and complete fitting

```powershell
python .\tools\probe-engine.py 'D:\VectorMagic\Vector Magic\vmde.exe' loader
python .\tools\record-fitting-samples.py 'D:\VectorMagic\Vector Magic\vmde.exe' cost
python .\tools\record-fitting-samples.py 'D:\VectorMagic\Vector Magic\vmde.exe' keepers
python .\tools\record-full-fitting.py 'D:\VectorMagic\Vector Magic\vmde.exe'
```

Mode 45 calls original 0x49e960 on the same explicit interval inputs as mode 44.
It captures squared residual cost, all eight initialized control-point doubles,
node kinds and retained random state. All 2,784 independent results match.
The branches contain 966 empty spans, 616 one-sample spans, 383 two-sample spans,
221 quadratic three-sample fits and 598 cubic fits with four or more samples.
The maximum interior count is 35. Short fits use quarter/three-quarter controls
but score only the first point against straight interpolation. The other
branches preserve regularized normal equations, original matrix/LU arithmetic,
residual order and quadratic-to-cubic coefficient conversion.

Mode 46 calls original 0x49f8b0 after its original work-array allocation. It
captures every active record's physical slot, relative start, span, next slot,
cached cost and dirty bit, plus the final threshold and PRNG state. Its 3,907
cases contain the 2,784 interval inputs, 84 iteration/fraction schedule probes
and 1,039 isolated threshold-multiplier probes. Empty intervals with initial
threshold one and one iteration isolate the stored multiplier without running
fit proposals. All records and thresholds match independently. No threshold
or cached-cost comparison is replaced with only a geometry comparison.

Assembly 0x49f8d5..0x49f90b uses FYL2X, native extended FLDLN2/FLDL2E constants,
53-bit divide/multiply/subtract boundaries, F2XM1 and FSCALE. Several profile
multipliers differ from Math.pow. The shared extended arithmetic reproduces
all 1,039 native values, including ratios adjacent to one and exponent-scaled
inputs. Common binary conversion/rounding helpers were moved from arctangent
support; the angle algorithm itself is unchanged.

Mode 47 imports the retained segmentation and supplies each completed original
smoothed graph, including positions, node kinds and region boundary flags. It
captures that supplied graph and verifies it is unchanged before calling
complete original 0x4a1030. It records initialized fitted geometry only: reserved
cubic-slot count, referenced cubic IDs/coordinates, node IDs/kinds and region
edges. All 2,712 completed upstream states match independently, including all
nine basic profiles. The one upstream smoothing fatal outcome has no geometry
to supply to fitting and remains retained in its smoothing corpus; no fitting
input was skipped. All 15 retained full-pipeline fitting targets also match.

The fitted-geometry wire reader is canonical in `tools/engine-reference-codec.py`
and is shared with the existing pipeline driver. `pipeline` now also writes
`test/fixtures/pipeline-fitting-reference.json` with its recording SHA. All four
fitting native binaries repeat byte-for-byte with the current helper; proof is
`evidence/fitting-native-repeatability-current.json`. Pass resolved Windows paths
to the native helper so reference DLL loading uses its verified installation.
The package contains none of these probes, recordings, original files or REA
outputs. Default-flag fitting does not establish output assembly or serializer
parity; the original extra refinement branch remains pending.

## Output assembly, SVG and runtime decimal formatting

```powershell
python tools/record-output-shapes.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/record-svg.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/record-cpp-formatting.py 'D:\VectorMagic\Vector Magic\vmde.exe'
```

Mode 48 captures original child/hole assembly, color order and visibility after
fitting, including hidden-bit word/range boundaries. Its 2,872 cases first verify
that original fitting still matches every retained input target. Shared codec
readers serialize initialized numeric fields only, never pointers or allocator
residue. Independent assembly matches all results and 112,496 visibility queries.

Mode 49 calls the dispatcher-confirmed SVG writer with three layouts, two stroke
policies, output dimensions and hidden words. One owned temporary file is read
only after the writer closes it. Its 17,436 byte strings cover 2,743 fitted
graphs, alpha cutoffs/opacity, dimensions, signed zero, decimal ties and large
finite coordinates. The original filename does not enter the output bytes.

Mode 50 calls the actual resolved MSVCR71 sprintf import with %0.2f. Its driver
reuses only the retained fixture input doubles; it regenerates every expected
string from the original. The 16,406 values span adjacent rounding boundaries,
all finite exponent ranges and random finite bit patterns. This isolates CRT
conversion from the stream's additional large-value scaling.

The stream converts using original 0x7c3c0f5f. Raw conversion follows sprintf
0x7c36a42b, __fltout2 0x7c372a35 and $I10_OUTPUT 0x7c371e14, with integer multiply
0x7c3730ed and decimal-power composition 0x7c37331e. Exact table observations
are retained in `evidence/msvcr71-decimal-power-constants.json`; the implementation
generates the effective powers mathematically. The original target was restored
after runtime inspection. Numeric and SVG recordings repeat byte-for-byte;
`evidence/output-native-repeatability.json` retains their hashes.

The pipeline driver also embeds all 15 original SVG files and SHA-256 values in
`pipeline-prepared-reference.json`. Source and installed public integration tests
compare every upstream result and final SVG without resetting the shared PRNG.
The package excludes the original files, recordings and development probes.

## EPS export and precision-three formatting

```powershell
python tools/record-eps.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/record-cpp-formatting.py 'D:\VectorMagic\Vector Magic\vmde.exe' 3
```

Mode 51 shares the fitted-graph/assembly/closed-file capture with mode 49 and
uses the original .eps dispatcher. It explicitly supplies operator style,
layout, stroke, dimensions and hidden words. The original dispatcher supplies
diagnostic flag zero itself (47d2b8); the capture uses the verified two-string
ABI. No original instruction or clock is patched.

The 41,010 EPS files cover 3,005 fitted geometries, every supported layout and
stroke choice, both compact/full operators, the alpha >50 cutoff, all byte RGB
values, visibility states and dimensions. Another 3,072 hidden byte-color
cases preserve the stream's initial general formatting before any coordinate
sets its fixed flag. There are 72 final-coordinate overflow files, while
extended intermediate exponents also preserve finite results where JavaScript
arithmetic would overflow too early. The independent mapper preserves the
original 53-bit subtraction/multiplication/division and final double storage.

Each EPS case retains its complete actual CreationDate value. Comparisons pass
that external metadata into the independent exporter and compare every byte.
The default local clock is checked separately without a mock. Native repeat
verification masks exactly 41,010 CreationDate payloads and compares the entire
remaining binary response, including supplied states, fitting, assembly,
outputs and PRNG fields. `evidence/eps-native-repeatability.json` records the
matching normalized digest. SVG, both decimal captures and original pipeline
numeric bytes repeat without a mask.

Mode 52 calls the resolved original sprintf import with %0.3f on 28,956 finite
doubles, including float32 byte colors and three-decimal tie/adjacent probes.
The original precision-two corpus stays unchanged. A separate development
capture retains the legacy infinity labels; the complete EPS files verify their
stream zero-insertion behavior at final-coordinate overflow.

The canonical EPS fixture is gzip-compressed with mtime zero at
`test/fixtures/eps-reference.json.gz`. Its generated CLI fixture keeps only the
small transport cases; SVG uses the same small CLI-fixture generator. Runtime
packaging excludes all fixtures and original reference material.

The EPS archive proof records its historical installation checks. Use the
current archive command at the end to verify the current source snapshot.

## PDF export

```powershell
python tools/record-pdf.py 'D:\VectorMagic\Vector Magic\vmde.exe'
```

Mode 53 dispatches the original PDF writer after original fitting/assembly and
captures complete closed-file bytes. The case factory deduplicates EPS-only
PostScript style choices and adds exhaustive alpha bytes plus explicit opacity
transitions. Its 22,014 files cover 3,264 fitted geometries. The shared recorder
uses Latin-1 JSON strings to retain every byte, including the binary header,
and writes `test/fixtures/pdf-reference.json.gz` with gzip mtime zero.

Default PDF output has no timestamp-dependent metadata. The entire native
response repeats byte-for-byte; `evidence/pdf-native-repeatability.json` keeps
its SHA-256 alongside SVG and the explicitly date-masked EPS response. The
pipeline fixture also retains every original PDF and its SHA-256. Public tests
compare prepared pixels through all three serializers in one retained session.
Binary CLI tests use raw process buffers so UTF-8 replacement cannot hide a
header or offset mismatch.

The PDF archive proof records its historical installation checks. The current
verifier includes AI and checks the current source snapshot as described below.

## AI export

```powershell
python tools/record-ai.py 'D:\VectorMagic\Vector Magic\vmde.exe'
```

Mode 54 dispatches the original AI writer after original fitting and assembly.
It reuses the PDF case factory: 22,014 files over 3,264 fitted geometries,
including 708 compound-path outputs and 90 scaled outputs. Complete ASCII
bytes, CRLF, header metadata, spaces and path delimiters are retained in
`test/fixtures/ai-reference.json.gz` with gzip mtime zero. No timestamp field
is replaced. The original AI header has an empty creation date.

The controller passes the same has-holes byte to shape begin and end. Exact
assembly establishes that contract where decompiler output is misleading.
Compound paths use u/*u and *U/U; hole starts paint the preceding subpath.
Header numeric fields set fixed formatting before RGB fields. Every captured
file is compared byte for byte. Public integration compares all 15 retained
prepared pipelines through SVG, EPS, PDF and AI without seed overrides.

The AI archive proof records its historical installation checks. The DXF coordinate archive records a later historical checkpoint. The current
archive also contains the complete verified DXF serializer described below.

## DXF coordinate and field kernels

```powershell
python tools/record-cpp-formatting.py 'D:\VectorMagic\Vector Magic\vmde.exe' 16
python tools/record-dxf-coordinates.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/record-dxf-coordinates.py 'D:\VectorMagic\Vector Magic\vmde.exe' special
python tools/record-dxf.py 'D:\VectorMagic\Vector Magic\vmde.exe'
```

Mode 56 captures the original CRT's %.16lf conversion. Its 28,700 finite strings
cover retained exponent probes and new precision-16 rounding boundaries. Mode
57 captures 126,984 original coordinate pairs, exact double bits and raw fields.
It also invokes the original field writer for every field below 256 bytes and
checks its actual group codes, CRLF and trimmed output. The request header adds
a length-prefixed UTF-16 output filename before the coordinate cases. Owned
512-byte storage captures the 98 overlong raw fields without invoking the
original writer's overflowing 256-byte buffer. Integer denominator wrapping,
negative divisors, signed zero, infinities and negative indefinite NaN remain
observable and receive exact comparisons. There are 109 nonfinite fields.

Mode 55 captures all three DXF curve modes and the shared export options.
Its 65,934 successful files cover 3,264 fitted graphs. Another 144 diagnostic
cases each retain an accepted visible line-end witness for an overlong numeric
field; these are not successful native file comparisons. Thirty-six successful
files retain those same extreme graphs with every region hidden. A tested full
original extreme export stalled, timed out and was terminated. No instruction
or clock is patched to avoid that behavior.

DXF headers use the full native palette, including entries absent from the
fitted byte colors. Each geometry record therefore retains the captured
normalized float32 BGRA palette. The new DXF API must receive that palette
alongside the fitted graph; it cannot reconstruct it from region bytes. The
default output-settings constructor uses DXF curve mode 2, dense lines.
Application preference loading is a separate remaining recovery boundary.

Portable file references live in `test/fixtures/dxf-reference/index.json` and
six gzip NDJSON parts. Iterate parts in manifest order while retaining indexed
geometry/palette records. Their concatenated logical stream has metadata,
geometry records before use, case records and a final count summary. Every
part checksum and the complete logical-stream checksum are verified. Part
targets are 32 MiB compressed and each is below 64 MiB, below GitHub's
[100 MiB file limit](https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github).
The full local native response is 3,423,079,368 bytes. Fresh processes repeat it
without any mask. Complete independent DXF serialization now matches all files.
The existing fresh-process repeat is retained: the original executable, helper,
full-file request stream and fixture did not change during this implementation.
The special-input mode separately captures 30 infinity/NaN coordinate pairs,
including quieting and payload/sign behavior, with 60 actual native fields.

The public API is `exportDxf` from `openvector/dxf`; the CLI command is
`openvector export-dxf`. Both require the retained palette and expose splines,
sparse lines and dense lines. Dense lines are the API default. The dispatcher
passes already oriented cubic controls and a separate direction flag. Native
sampling observes both; the canonical walker and all five writer callbacks now
follow that ABI. Every previous writer still passes its complete byte corpus.
All 15 retained public pipelines compare all five outputs using their actual
segmentation palette and shared session state without seed overrides.

Run the current installed-package verifier from its fresh installation directory,
passing absolute script/archive paths:

```powershell
Set-Location 'D:\migracion\ambivalence-verification\emf'
node 'D:\migracion\vector-magic-re\tools\verify-output-package.mjs' 'D:\migracion\ambivalence-verification\emf' 'D:\migracion\vector-magic-re\artifacts\emf\ambivalence-0.0.0.tgz'
```

The verifier enforces that actual working directory, resolves every runtime
module from the installed package and compares every archive/source/installed
byte. Source paths supply recorded fixtures and receive evidence only.

## EMF native files and integer coordinates

```powershell
python tools/record-emf.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/record-emf-coordinates.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/record-emf-device-bounds.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/record-emf-expanded.py 'D:\VectorMagic\Vector Magic\vmde.exe'
```

Mode 58 shares original fitting, shape assembly, settings and exact closed-file
capture. It also records read-only GetGuiResources counts. The initial 60 files
repeat exactly, and measurement leaves all their bytes unchanged. The original
retains created brushes/pens across exports: that initial process grows from
zero to 54 GDI objects. The complete recorder therefore uses fresh processes
with at most 2,048 possible new objects and 128 files per batch. It does not
change original calls or delete its objects. A single file above that budget
fails explicitly. Use an empty evidence/emf-native-batches directory for a new
capture; existing evidence is preserved rather than overwritten.

All 22,014 complete files across 3,264 fitted graphs were captured in 656 batches.
Every batch runs twice and its whole raw response must repeat without a mask.
The manifest retains each request/response digest, size, case range and measured
object counts. The aggregate digest covers the ordered complete raw responses,
including every process prefix; it is not a single-process response digest.
The canonical fixture is test/fixtures/emf-reference.ndjson.gz. It retains every
file byte bijectively through Latin-1, with indexed geometries before use and a
terminal summary. The complete JSON document exceeded Node's string-size limit;
its original gzip remains in evidence/emf-full-native-reference.json.gz. The
streamed representation preserves every native file byte and digest. The
completed helper also preserves every raw response in all 656 batches.

Mode 59 calls original point helper 0x4780f0 with the scale from 0x8ddb08. It
captures 277,909 pairs over fitted nodes/controls, every finite exponent,
adjacent 16/32/64-bit storage boundaries, signed zero and infinity/NaN patterns.
The full native response repeats exactly. The independent coordinate function
matches every signed int32 result. Native arithmetic truncates into signed
64-bit storage and keeps the low word; overflow/invalid stores have low word zero.

The EMF header carries source-device dimensions, as described in Microsoft's
[Header Object specification](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-emf/e4a35c41-e8e3-43f9-bc07-a18e99bb866d).
Captured files use 1920x1080 pixels, 527x296 millimeters and 527000x296000
micrometers. Their 64-character description includes adjacent original static
data after the intended label. All bytes are retained. Original source-sized
and changed-output-dimension exports match exactly for the captured options.
Environment behavior remains a separate recovery boundary.

Exact original imports identify EndPath followed by FillPath. Microsoft's
[FillPath documentation](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-fillpath)
confirms that the operation fills the current path with the current brush.
The observed resource lifetime is also consistent with Microsoft's separate
brush/pen destroyer functions in [GDI Objects](https://learn.microsoft.com/en-us/windows/win32/sysinfo/gdi-objects).
The provider search and extraction results are retained in
evidence/emf-spec-research.json.

Mode 60 isolates the resolved x86 GDI backend with stock solid-brush paths.
All 93,880 original mapping/frame pairs repeat and match independently. Inputs,
scales and products round to float32; conversion to signed 28.4 uses nearest
with ties away from zero, followed by pixel rounding using (fixed+8)>>4.
Header conversion is MulDiv(int32(bound*100), millimeters, pixels). The wrapped
first multiplication matters at large coordinates. Microsoft's
[MulDiv documentation](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-muldiv)
specifies its signed half rounding and overflow result; the wrapped caller
operand is established by native probes. Research is retained in
evidence/emf-frame-spec-research.json. The earlier failed arithmetic hypotheses
remain evidence rather than fallback code.

Mode 61 shares original fitting, assembly and EMF dispatch while substituting
explicit source dimensions only after fitting. Its 402 repeated files cover
wide cubic records and int32 source-extent boundaries. All bytes match the
independent writer. The original 22,014 full files, all 490,530 filled-path
bounds, empty cubic bounds, source/output dimensions, groups, visibility,
alpha cutoff, object IDs and metadata also match.

The API is exportEmf from openvector/emf; the CLI command is export-emf. Both
require explicit referenceDevice pixels/millimeters/micrometers pairs. The
serializer uses no GDI or original application at runtime. Unrepresentable
fixed-point coordinates and 16-bit object counts fail clearly. All 24 retained
binary CLI cases and all 15 public pipelines through six formats pass. Automatic
environment acquisition and complete application behavior remain unfinished.

## Prepared-image palette-count analysis

Mode 62 executes predictor 0x4abf40 with controlled 62-double vectors and original
label registries. The 31,604 inputs include threshold neighbors, ten profile
names, infinities and NaNs. Both fresh-process responses repeat without masking.

Mode 63 prepares BGRA grids and original neighbor directions with 0x472cb0,
executes 0x48f2a0 for every count 2..12 in one retained session, and captures
palettes, maps, membership weights, raw float scores and every PRNG transition.
It then captures histogram peak weights, all 62 double feature bits, and the
original prediction and decoded label. The 576-case corpus includes 270 completed
images and 306 empty-peak native faults. A fault ends that native session; the
next case starts a new process. Both original fault contexts are retained, while
only the instruction/code/stage are compared because register pointers vary.
All completed bytes before each fault compare without masking.

Mode 64 executes original histogram, refinement and scoring kernels before
quantization, using the same session boundaries. All 6,336 maps, weights, scores,
PRNG states and independently quantized palettes match complete mode-63 results.
The earlier recordings with uninitialized neighbor directions remain in
evidence/classification-uninitialized-directions with the demonstrated harness
limitation. They are not application-parity evidence.

```powershell
python tools/record-classification-model.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/record-classification-features.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/record-classification-features.py 'D:\VectorMagic\Vector Magic\vmde.exe' raw
node --test test/automatic-colors.test.mjs
```

## Image type/quality reference stages

Mode 65 calls 0x4ac470 with twenty raw float32 features and a 28-byte by-value
short extension string, then uses original type/quality registry lookups. The
57,472 cases cover all five emitted labels, 65 threshold comparisons, float32
neighbors, exact nonfinite bits, 22 strings and the literal-extension branch.
Both complete responses repeat byte-for-byte. Its recorder follows the native
CFG with a symbolic x87 operand stack when extracting thresholds; prefetched
operands can cross branches and must not be paired by a linear scan.

Mode 66 constructs a Magick image through 0x472c30, loads it through 0x473bb0,
sets explicit native preprocessing controls and prepares it through 0x4738b0.
It follows classification thread 0x46e910 by setting palette mode zero and
calling 0x488d20 with (1,0,0) before the feature producer 0x491d40. Numeric
parameters are fresh constructor values, not asserted GUI defaults. The
96 cases contain 72 complete feature vectors and 24 empty working-image
outcomes from original EnhanceImage on small rasters. No synthetic label
is produced for an empty normalization result.

After each completed feature result, a separate native blur/gradient repeat
records the blurred BGRA and two float32 maps; the probe verifies unchanged
PRNG and restores the original freed gradient-cache state. Contrast bytes,
connected labels, region moments, numerical controls and working pixels are
also retained. The derivative calls do not replace any original feature
processing instructions.

```powershell
python tools/record-image-classifier.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/record-image-features.py 'D:\VectorMagic\Vector Magic\vmde.exe'
node --test test/automatic-colors.test.mjs
```


## Prepared classifier blur and complete shadow dispatch

Modes 71/72 recover the two shadow decision models and principal-color iteration.
Modes 73..78 use hardware entry/return breakpoints to capture actual principal
state, shadow proposals, wider subpixel rebuilds and acceptance. Original
instructions remain unchanged. The explicit refinement corpus includes multiple
simultaneous shadows and accepted/rejected restoration paths.

Mode 79 captures every original Gaussian kernel during complete classification;
its full responses equal untraced mode 66 after the helper-layout header. Mode 80
captures initial alpha preparation and fixed Gaussian output on 2,224 rasters,
including all alpha/color-byte pairs. Working pixels are compared before and
after blur; raw input is retained separately because initial preparation changes
RGB. Mode 81 extends constructor context with the exact int32 acceptance control
at core 0x298 while preserving the original 64-byte context block.

Modes 82/83 capture complete prepared segmentation under automatic/supplied
palettes, explicit shadow/cluster switches, all 100 numerical parameters and
acceptance controls. The Q8 image cache receives already prepared BGRA directly,
preventing a second initial alpha multiplication. Traced/untraced outputs and
both fresh repeats are identical, including all pixels and PRNG states.

```powershell
python tools/record-shadow-models.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/record-principal-colors.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/record-shadow-proposals.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/record-shadow-refinement.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/record-classifier-blur.py 'D:\VectorMagic\Vector Magic\vmde.exe'
python tools/record-image-features.py 'D:\VectorMagic\Vector Magic\vmde.exe' --context
python tools/record-shadow-segmentation.py 'D:\VectorMagic\Vector Magic\vmde.exe'
node --test test/automatic-colors.test.mjs test/segmentation.test.mjs
```

## Enhance preprocessing

Mode 84 enters the prepared Q8 cache directly and calls the original
EnhanceImage wrapper at 0x4ae620. It preserves input bytes and the PRNG, records
zero dimensions for small-image empty results, and exports successful results
through the original BGRA adapter. Mode 80 also records complete mode-one
preparation with an explicit zero through three passes, initial alpha handling,
and subsequent Gaussian blur. Two fresh native responses must match exactly.

The 4,208 kernel cases include 1,728 cutoff-adjacent images. Integer distance
numerators select test inputs immediately below, at and above the mathematical
cutoff; the implementation retains native floating-point operations and uses
captured outputs as its authority. Another 1,616 cases cover whole preparation.

```powershell
python tools/record-image-enhance.py 'D:\VectorMagic\Vector Magic\vmde.exe'
node --test test/image-preparation.test.mjs
```

## Nonpeak median, despeckle and preprocessing dispatch

Mode 85 calls the original median/despeckle wrappers over already prepared Q8
pixels. It records successful output bytes, unchanged inputs and PRNG state.
For Magick::ErrorOption, the exception observer also reads the original message
string before unwinding. The recorded `engine` client-name prefix belongs to
this native reference context; application thread/UI handling remains separate.

The new recorder verifies 10,186 direct kernels, 3,839 mode/amount dispatches and
176 size errors. Successful complete native responses repeat byte for byte;
pointer-bearing failures compare type, message and stage while retaining both
raw captures. The median corpus includes every three-level histogram population
for all supported kernel sizes; despeckle includes all binary 3-by-3 fields,
one-dimensional rasters, opacity and low-contrast directional fields.

Modes 66/81 then capture 123 complete classifications after modes 2/3. Stripping
the separately observed constructor contexts must reproduce the untraced
feature payload exactly. Tests run independent preprocessing before comparing
all feature, region, blur, gradient, contrast and classification outputs.

```powershell
python tools/record-image-filters.py 'D:\VectorMagic\Vector Magic\vmde.exe'
node --test test/image-preparation.test.mjs
```

## Q8 resize and working reduction

Mode 86 records original SampleImage and filter-3 ResizeImage over already
prepared Q8 pixels, with explicit source/target dimensions and matte flags.
Both controls are inputs. Output bytes, unchanged source bytes, blur context,
dimensions and PRNG must repeat exactly. Mode 87 follows the real image setter,
preprocessing and working reduction above 200,000 pixels; it records both the
working raster and the full retained preprocessed source cache.

Mode 88 constructs the original Geometry object and invokes its original text
producer before ParseMetaGeometry. Zero dimensions are omitted by that producer;
this differs from a literal `0xH` string. The private implementation follows the
actual producer used by working reduction. Geometry fitting retains a double
width ratio but stores its height ratio as float32; one-axis requests have a
different integer-to-float32 storage point.

The recorder compares 12,084 resizes, 20,056 geometry cases and 92 full working
preparations. Every native response repeats byte for byte. Mode 89 additionally
captures five source-cache blur and working-gradient states. The independent
private source/working pipeline compares all of these maps exactly.

```powershell
python tools/record-image-resize.py 'D:\VectorMagic\Vector Magic\vmde.exe'
node --test test/image-resize.test.mjs
```

## Source and working classification state

Mode 90 shares the original image setup and complete type/quality producer with
mode 66, while recording full source-cache dimensions/pixels separately from
the working raster. It accepts an explicit anti-alias control and captures the
actual dispatch context, all 20 feature bits, final classification, source blur,
working gradients, contrast, region labels and ordinary moments. It preserves
original instructions and verifies unchanged working bytes after analysis.
Both complete recordings must repeat byte for byte.

The 16 inputs cover solid and four-color blocks, transparency, all threshold
sides, noise reduction/despeckling and both anti-alias controls. Twelve inputs
reduce their working dimensions. Product comparisons independently prepare,
resize, segment and classify the decoded input; native intermediates supply
expectations only. Automatic profile application and cache invalidation remain
outside this fresh-state experiment.

```powershell
python tools/record-image-source-state.py 'D:\VectorMagic\Vector Magic\vmde.exe'
node --test test/image-source-state.test.mjs
```

## Complete automatic image-analysis controller

Mode 91 shares the original decoded-image setup, then calls 0x4933e0 after
classification-thread segmentation. It records both type/quality and palette
labels, all image/palette feature bits, Shared control changes, source cache
pixels/processed flag, initial/final working pixels, all retained candidate
palettes/weights/recommendation fields, the final assignment map and PRNG.
Original instructions remain unchanged; no native model is pre-run for setup.

The 44 inputs cover all four preprocessing modes, both initial anti-alias
controls, transparency, solid blocks, noisy blocks and photographic noise.
Eight images undergo working reduction. All five emitted image labels, all
three palette classes and 18 anti-alias changes occur. The adapter produces
matte-one source contexts; original file/non-matte loading remains a separate
recovery task.

Two inputs trigger the native empty-peak access violation at 0x48cac2. The
recorder retains them and resumes later inputs in another fresh process, giving
three sessions. Successful payloads and semantic fault payloads repeat exactly;
fault code, instruction and stage also agree. Volatile exception register
addresses remain raw evidence and are not byte equality expectations. Native
PRNG effects are captured before the failed process exits. The library preserves
them while returning its explicit empty-peak diagnostic.

```powershell
python tools/record-image-controller.py 'D:\VectorMagic\Vector Magic\vmde.exe'
node --test test/image-controller.test.mjs
```

`CoreSession.analyzeImage` and `analyze-image` use the same independently authored
controller. The library retains raw palette scores/features and reports native
recommendation writes separately. CLI comparisons use fresh-session native
results, matching file/stdin input, help and error/no-partial-output behavior.


## Document cancellation reference

Operations 97 and 98 observe the original direct palette task and actual Qt
palette worker. Cancellation invokes original flag method 0x4690d0. The hardware
observer records every refinement checkpoint; the Qt observer records changed
integer percentages with an explicit initial previous-notification value of
zero. Unhandled CancelTaskException ends these development contexts with CRT
exit 3. This does not establish full GUI cancellation behavior.

Operation 99 wraps the original task in a development-only external caller
catch. First-chance and post-unwind document states must agree exactly before
retrying in the same session. Source-cache export uses a separate temporary
native grid and never overwrites live engine scratch. All complete responses
repeat exactly before entering the fixture. The helper stays frozen throughout
recording, uses recovered Qt priority 2 and keeps original frame handlers
active. Status 0xffffffff in throw snapshots is the recorder's no-return
sentinel, not a native method return.

The corpus covers 234 cases / 498 steps, including 144 cancellation exceptions
and 48 caller-caught retries. Native evidence and final frame equivalence live
under document-cancellation-*.json. Node tests consume only the independent
implementation and fixture. The installed verifier also denies original-file
reads and child creation. Original GUI handlers, late cancellation/status 2
and general failure completion remain outside this comparison.


## Complete document segmentation profiles and cache transitions

Operation 100 accepts explicit palette and segmentation edits on the constructed
original document. Its one typed registry descriptor list records all 100
parameter fields after each action. It records the actual live region-grid
dimensions rather than assuming they equal the full source. Original palette
vector assignment owns input-color copies; original source preparation,
profile loading, cache checks and segmentation execute unchanged.

```powershell
python tools/record-document-segmentation.py 'D:\VectorMagic\Vector Magic\vmde.exe'
node --test test/document-segmentation.test.mjs test/settings.test.mjs
```

The corpus has 113 actions, 81 segmentation key misses and 20 hits. Replaying
palette work between segmentation requests exposes shared map/palette mutation
alongside retained region metadata. These states must guide the eventual
independent document model; the current profile test does not claim that API.

The complete profile comparison also proves shadow dispatch at +0x214 is
distinct from registered diagonal extraction at +0x930. The settings arithmetic
reader now labels the former shadowing. Its 648 raw captures and every numeric
value stay unchanged. Re-recording/generation preserves numeric production
tables; only the corrected fixture label and resolver behavior change.


## Unified image document and corrected key identities

Operation 100 now records 145 actions on four decoded sources: 16 palette
requests, 93 segmentation recomputations and 36 segmentation cache hits. Every
fresh original response repeats byte for byte. The extra actions cover supplied
palette restoration before a key hit, rounded palette/background key collisions
and flexible-mode palette edits ignored by the segmentation key. The earlier
113 input actions and their original oracle remain retained under evidence.

The canonical independent `CoreSession.createDocument` computes its own source
pixels, settings, candidate palettes and segmentation. Tests compare complete
live assignments/palettes, retained region metadata, all 14500 typed parameter
values, settings, hidden switches, source/working rasters, cache keys and PRNG.
The installed verifier repeats these checks with original-file reads and child
creation denied. No native intermediate is injected into the decoded workflow.

REA 809..827 corrects the development decoder's stage names: document +0xf8 is
segmentation edits, +0x114 smoothing and +0x130 fitting. Preserve +0xc0 as
`reservedC0` because its stage role is not established. The native response
bytes and all state values stay unchanged. The original raw recording hash and
the label-only audit are retained in image-document-key-identity-verification.

The complete smoothing and fitting document wrappers have now been traced,
including their success continuations and settings-key dependencies. These are
static findings, not verified document implementations. Their capture contract
is in image-state-contract.json under documentVectorStageRecovery. Existing
prepared smoothing/fitting corpora do not establish their document lifecycle.


## Complete document smoothing, fitting and assembled chains

Development-only operation 101 extends the canonical operation 100 wire format
with explicit smoothness, corner detection, curve complexity and smoothing
anti-alias controls. The latter is distinct from palette anti-alias. Original
constructor fields establish smoothness and curve complexity as 6. Capture
readers now accept the actual core context instead of assuming the standalone
probe's core. Their old operation 100 response remains byte-identical. Fresh
before/after helpers also agree for all 2712 prepared fitting cases; the much
older historical recording differs only in the helper entry-address header.

The oracle records 378 actions on eight sources and repeats every response byte
in a second fresh native process. It covers all nine basic profiles, advanced
smoothing/AA/corner settings, curve-only edits, palette work between vector
hits, nested shape chains and full-resolution/reduced state on a 512x512 image.
No original instructions are patched and no native intermediates are supplied.

```powershell
python tools/record-document-vector-stages.py 'D:\VectorMagic\Vector Magic\vmde.exe'
node --test test/document-segmentation.test.mjs
```

Independent naive composition matches 77 smoothing runs / 85 hits and 113
fitting runs / 45 hits, complete geometry/metadata/assembly, 37800 typed
parameter records and session PRNG. The actual ImageDocument API then makes
its own cache decisions and passes the same complete-state comparisons.
Fitting mutates the shared contour node kinds seen on later smoothing hits.
Both corpora now enforce 523 actions and 52300 parameter comparisons. The
restricted installed verifier denies original-file reads and child creation.
Manual edit commands, supplied vector-stage palette edits, vector cancellation
and errors, final GUI palette selection and original document export actions
still require separate captures. Full source test results and archive pins are
recorded under document-vector-stage-*.json.


## Actual Qt controller requests and skipped-stage commits

The current recorder accepts `controller` or `controller-pilot` to select
operation 102. It constructs the original ThreadManager, original document and
QCoreApplication, starts original priority-2 workers and pumps Qt events until
the real queued finished() callback completes. Requests explicitly disable
automatic continuation and leave the unused GUI-model context null. No fake
MainModel, manual taskFinished callback or original-instruction patch is used.

Original CRT stubs 0x6f8c10 and 0x6f8c90 initialize the imported QThread/QObject
parent metadata. The failed pilot without these stubs reported a missing
FindPalettesThread::finished() signal and was rejected. Correct initialization
restores actual Qt signal delivery. Undefined tiny Ghidra metadata procedures
were recovered by observed-address bytes and LLVM disassembly, not guesses.

The accepted corpus repeats all 384 requests on eight sources: 243 computed
and 141 skipped. It captures active/pending/requested/committed settings, native
completion state, all typed parameters, full pixels, shared geometry and chains.
Skipped palette work copies requested settings into the document. Other skipped
stages preserve earlier committed settings and the last core profile. The new
ImageDocument.runTask API independently reproduces these completed semantics;
its direct stage methods retain their separate native profile-before-key rules.

Both earlier operation 100/101 recordings remain wholly byte-identical. Current
installed verification enforces all 907 direct/controller actions and 90700
typed parameter comparisons with original-file reads and child creation denied.
Automatic chaining, busy replacement, controller cancellation/errors and full
GUI/default flows remain separate capture requirements.

## Native document classification

`tools/record-document-classification.py` records operation 104 through actual
ClassifyImageThread/ThreadManager kind 1 and Qt queued completion, followed by
explicit interleaved palette/segmentation/smoothing/fitting requests. Both fresh
responses match byte for byte: 19 sources, 380 actions, 133 classifications,
361 computed and 19 skipped requests. The fixture includes full recommendations,
all 20/62 image/palette features, eleven candidate palettes/weights/scores,
typed Shared controls, registered profiles, live pixels and valid geometry.

Supply source shadow context explicitly. Shared constructor 0x4723d0 does not
initialize +0x90; the failed operation 103 experiment is retained. Capture the
Shared +0/+8 byte booleans as typed values, excluding undefined adjacent padding.
The failed raw-padding recordings are also retained. Geometry keys determine
initialized views; allocated raw storage counts are captured separately.

Independent document comparisons now cover 1287 interleaved actions and 128700
typed parameter records. The new recorder replays all prior operation 100/101/102
requests without changing their accepted bytes. A fresh 44-file private package
passes five durable document tests while original/workspace engine reads and
child creation are denied. Source/packed/installed bytes agree.

The fresh 73-file GUI preview passes three real integration tests and contains
no node_modules. Node 24.12.0 preserves original-read/child-process restrictions
in a worker probe but permits workspace source reads. Worker filesystem
isolation is not established. Do not generalize main-thread permission checks
to workers. See document-classification-verification-checkpoint.json for exact
scope, artifacts and the retained full-suite Adam7 failure.

## Segmentation editing reference

Operation 112 uses the real MainModel slots, editor page 14 and its QImage.
It sends `takeAction(ToolActionRecord,bool)`, undo, redo and revert requests,
then leaves through `processNextPress()`. The reference window receives normal
Qt show/layout events with `WA_DontShowOnScreen`; the Next handler's visibility
guard remains intact. No original instruction or visibility flag is patched.
The enum reference is [Qt's public header](https://github.com/qt/qt/blob/4.8/src/corelib/global/qnamespace.h).

```powershell
python tools/build-native-reference.py C:/Users/Microck/scoop/apps/llvm/current/bin gui
python evidence/record-native-gui-edits.py
python evidence/record-native-gui-edits.py palette-reorder
python evidence/record-native-gui-edits.py fill-contact
python evidence/record-native-gui-edits.py color-rounding
python evidence/record-native-gui-edits.py flexible
python evidence/verify-native-gui-edit-helper-regression.py
python evidence/create-segmentation-edit-fixture.py
```

The declared scenarios also cover history ordering, pencil lines, QColor
precision, zap ties, blended artwork and semitransparent paint. Eleven native
responses repeat exactly in two fresh processes: 94 actions, 105 editor states
and 11 completed refits. Ten refits have finite geometry. One completes with
non-finite coordinates; expected values retain explicit IEEE tags. A separate
alpha stress recording fails repeat equality after smoothing/refitting, even
though its initial processing, edited pixels and histories agree. Its failed
raw pair remains evidence, not a passing conformance case.

`read_gui_editor_session` is the canonical decoder. Fixture generation reads the
raw recordings before JSON encoding, keeping numerical ground truth separate
from presentation. The corpus contains own encoded inputs and expected output;
recorded intermediate images must never become computation inputs. Independent
initial processing and editor-image preparation match all eleven cases while
original executable reads and child creation are denied. Post-edit independent
behavior matches all 105 editor states and ten finite refits. The alpha-heavy
non-finite result remains a strict failing comparison.

The extended helper preserves thirteen earlier native workflows byte for byte
across 26 fresh processes. Original installation files and saved numeric
profiles remain unchanged. See [the editor contract](segmentation-edit-contract.json)
and `../evidence/segmentation-edit-reference-checkpoint.json` for scope and gaps.

## Native pointer gestures and paint selection

Operation 113 observes the actual `VmMainView` owned by `MainWindow`. Qt runtime
metadata establishes its class and 81 inherited/local method signatures. REA
requests 1007 through 1017 recover pointer handlers, world-coordinate mapping,
record construction and the initial palette-color connection. The handler at
`0x446020` receives moves; `0x4463a0` receives presses.

The original handlers read `QCursor::pos()`. The recorder maps each declared
image point through the actual scene/view, temporarily positions the cursor,
dispatches a real constructed `QMouseEvent` through `QCoreApplication::sendEvent`,
and restores the cursor immediately. It sends no OS mouse-button or foreground
events. Every successful frame asserts requested/observed world coordinates and
cursor restoration. The reference window uses `WA_DontShowOnScreen`.

Sixteen own-input workflows repeat byte for byte in fresh native processes:
177 actions, 193 editor states and 15 completed document states. The two
paint-only alpha workflows have no Next result. A dominant-ink witness proves
that initial paint follows palette entry zero rather than the first pixel.
Picker release samples edited segmentation, preserves captured alpha values
with Ctrl/Shift/Alt/Meta, and restores the previous tool. Each changed Pencil
move adds history; release adds none. A short-stroke witness also establishes
the native one-sample `(0,0)` painter coordinates.

`evidence/create-segmentation-gesture-fixture.py` verifies recorded pairs and
generates the assertion corpus. Expected pixels, commands and document state
never enter the independent engine. Source/core and CLI tests pass 53 checks;
the source and freshly extracted GUI each pass 33. An installed package with
original/workspace reads and child creation denied matches all sixteen gesture
workflows and fourteen CLI output plans. The combined installed editor suite
retains the known alpha-heavy failure. The full source suite reports 325/327.

The final helper reproduces 46 earlier complete responses. The harness then
failed its temporary-registry cleanup check; cleanup was recovered and all 46
retained response hashes revalidated. A second regression run hit the recorder
deadline during the large-texture worker trace. Preserve that partial response
and both failures; do not describe either entire harness run as green. These
details and the unchanged 43 installation files appear in
`evidence/segmentation-gesture-native-regression-verification.json`.

The private artifact, browser check, simplification review and all verification
logs are recorded by `evidence/segmentation-gesture-verification-checkpoint.json`.
These are bounded matches. Complete gesture, editing and application parity
remain unfinished.

## Background editing and Qt offscreen viewer observation

Operation 114 extends the owned reference workflow without changing operation
112/113 wires. It records logical hidden bits, each original visibility getter,
ordered region-ID history, selected tools and settings, bitmap edits and staged
vector availability. Action 21 calls the original model's world-point action;
action 22 supplies declared tool-record inputs; neither moves the cursor.
Existing successful pointer recordings establish the actual widget producer.

Advanced Next can commit edited pixels while regions have no boundaries, despite
retained node allocations and nonempty cache strings. Action 23 records current
region/node/boundary extents before optional completed geometry serialization.
Normal completed captures retain their strict nonempty-boundary contract. The
root cause and rejected observer assumptions are preserved in
`evidence/background-staged-capture-root-cause.json`.

Action 24 waits for Qt worker completion, then uses public QPixmap.grabWidget
and QImage PNG output on the owned hidden MainWindow. It records window-mapped
samples and PNG files without desktop capture. Pixel samples repeat; a few whole
window PNG bytes can differ during intermediate repaint, so no entire-window
pixel equivalence is claimed. The alpha-outline and color-lifetime recordings
repeat as whole binary responses and preserve original files, profiles and the
isolated registry cleanup guard. Failed cursor-restoration and staged observer
captures remain retained and excluded from successful fixtures. An intercepted
native first-chance texture exception does not establish that the normal GUI
crashes or fails to complete its error handler.

The current helper is e474e5f88e5eab59217e30b9fb8fe6702aa9ade1e7f5881988c26b35c9591b43.
A fresh operation-113 dominant-paint regression produces the same entire response
as its prior helper in two fresh processes, with all guards passing. This does
not rerun the older 46-response regression corpus. Current fixtures, runtime
checks and remaining gaps appear in
`evidence/background-verification-checkpoint.json`.

## Native viewer keyboard and layer state

Operation 114 actions 25-30 construct real public Qt key/focus events, dispatch
them to the owned VmMainView, observe Primary/Base/Current layer IDs and actual
widget guards, and invoke declared view/model slots. No OS keyboard injection,
foreground activation or cursor movement is used. Six sequences repeat as whole
responses in two fresh native processes, covering 99 actions. Original files,
saved numeric profiles and isolated-namespace cleanup guards pass.

The key-handler entries are 0x44b350 and 0x44b470. Ghidra has no procedure records
for them; raw LLVM assembly and actual Qt dispatch supply the evidence. The
initial release-address query at 0x44b480 was rejected and is not a recovered
entry. QKeyEvent::modifiers flips the modifier-key bit from the constructor's
stored state. Captures retain this producer value independently of the inputs.
Layer override writes MainModel +0x29c; +0x294/+0x298 are Primary/Base. B/V flags
can clear on another key press while the current layer waits for a release to
return to Base. Temporary Pan retains its old restore tool even after selecting
Pan. Direct focus-in/out preserves the captured state; normal OS focus routing
and Qt shortcut activation remain separate work.

The large MainWindow constructor exceeded REA's response budget, then its entire
159,811-character pseudocode was recovered through the retained-evidence view
with exhausted coverage. It connects three Auto-Update checkboxes to palette,
segmentation and contour update slots. Constructor 0x412b90 establishes checked,
initially disabled, 500 ms single-shot timers. Original QtCore's exported getter
independently verifies the timer bit. Runtime timer sequencing is not yet
established. See viewer-input-contract.json and auto-update-contract.json.

## Native batch queue

The separate `tools/probe-batch-native.c` helper uses operation 116. Count zero
captures MainModel and BatchModel metadata. Nonzero counts run typed queue
actions and capture initialized filename/settings fields, button indices and
three initialized record flags. Strings use explicit UTF-16 code-unit lengths.
Action 1 splits a NUL-delimited QString into a real Qt QStringList, then transfers
its ownership to the original add method at 0x404930. The original method frees
the by-value argument, as the native MOC does. No synthetic Qt list headers or
reference-count manipulation is used.

Actions 0-10 observe state, add filenames, clear, delete by index, set directory,
set format, set processing index, set editability, switch batch mode, select a
record through QGraphicsItem, and delete selected records. Duplicate notification
uses the original Close button (0x200000), rather than substituting a return.
The original persistent thumbnail thread runs at 0x436fc0. Settled observations
acquire its BatchModel mutex and wait for all attempted flags. The first racing
recordings and the rejected OK-button assumption remain retained.

Eight recording pairs cover 48 queue/settings states, including a metadata
regression against the earlier helper and a whole-response settled-queue
regression. Verify them with:

```powershell
C:\Python314\python.exe evidence/verify-native-batch-queue.py
```

`evidence/record-native-batch-queue.py` supplies the recorder and full original
settings-tree guard. It writes only fresh evidence names and cleans only the
owned isolated namespace. The input desktop and all 43 original installation
files remain unchanged. Private registry values enter only an in-memory hash.
Inactive invalid filenames are queue observations; active malformed-thumbnail
error completion remains unverified because the oracle intercepts first-chance
C++ exceptions. No batch conversion, output naming or cancellation parity follows
from these queue captures. See `docs/batch-contract.json`.
