# Node API and CLI

## Use the npm package

The package root exports the Node-API classes and functions:
`CoreSession`, `ImageDocument`, `BatchRunner`, `SourceHistory`,
`SourceDirectories`, `SavedAdvancedProfiles`, `RecentBasicProfiles`,
`decodeImage`, `defaultDocumentSettings`, `defaultAdvancedProfile`,
`defaultBatchProfile`, `batchProfileControls`, `applyAdvancedProfile`,
`applyBasicProfile`, `basicProfileLabel`, `applyPaletteCommand`,
`resolveParameters`, and `formatFileSize`. The default export is the addon
object. `npm/index.d.mts` declares these exports and their result shapes.
The package has no subpath exports.

```js
import { readFileSync, writeFileSync } from 'node:fs';
import { CoreSession } from 'openvector';

const session = new CoreSession();
const document = session.createDocument();
document.setSource(readFileSync('logo.png'), 'logo.png');
document.runAutomatic({ extension: '.png' });
writeFileSync('traced.png', document.exportBitmap('png', 2));
// saveVector creates a new file and never overwrites an existing path.
const saved = document.saveVector('traced.svg', { layout: 'cutouts' });
if (saved.status !== 0 || saved.error) throw new Error(saved.error?.message ?? `status ${saved.status}`);
```

Processing calls are synchronous. Create documents and batch runners from one
`CoreSession` so they share its retained random stream.

### Load and process images

`setSource(bytes, filename)` decodes PNG, BMP, JPEG, GIF or TIFF bytes. It
opens no file; the filename's suffix only sets the source's shadow-processing
context. `decodeImage(bytes)` decodes without a document and returns
`{ width, height, bgra, hasAlpha, format }`.
If `setSource` fails, the document retains its previous image, settings,
processing results and export eligibility. Failed Open in the GUI also keeps
the previous image. This preservation is an intentional difference from the
measured Vector Magic failed-open behavior.

`document.shrinkSource()` explicitly reduces a source above 1.6 megapixels
using the native block-average operation. It returns `true` when it replaces
the source and clears derived processing/editing state, while retaining
committed controls, the session random state and the source alpha
classification of the full-size raster (as native does). Otherwise it returns
`false` without changing state. Call it after `setSource` and before processing.
Neither method rewrites the input file.

`runAutomatic(controls, progress?)` classifies the image, chooses a palette
and background, then segments, smooths and fits. Unspecified controls come
from the document's committed settings. `controls.extension` is required and
holds the filename suffix with its dot. The classifier compares it exactly
with `.bmp`, `.jpeg`, `.png` and `.tiff`. The CLI derives it from the filename
and reads `.jpg` as `.jpeg` and `.tif` as `.tiff`, so pass the same value to
match CLI results. The result holds the selected `settings`, `paletteCount`
(`null` when no palette search ran), `sourceAlphaType`,
`recommendedBackgroundMode`, the final compact `state: DocumentSummary`, and
`fitting: { status, didWork }` from the fitting stage.
`runBatchImage(controls, progress?)` applies the settings described under
[batch conversion](#batch-conversion), plus `extension`, to the loaded image
and returns the same shape. See the [automatic contract](automatic-contract.json).

`runTask(stage, settings, progress?)` runs one stage and returns
`{ didWork, settings, state }`. The stages are `classification`, which needs
`extension`; `palette`, which needs `lower` and `upper` from 2 through 24;
`segmentation`; `smoothing`; and `fitting`. A stage whose cache key already
matches returns `didWork: false`. Classification stores its recommendations
in `state.classification` without applying them. `commitSettings(settings)`,
`snapshot(selection?)` and `reset()` manage the document, and `defaultDocumentSettings()`
returns the default document settings. See the
[image state contract](image-state-contract.json).

The optional progress argument is `{ callback(event, checkpoint), capture? }`.
The callback runs at Rust checkpoints and returns `true` to continue or `false`
to cancel. `capture` uses the same `SnapshotSelection` as `snapshot(selection)`.
Omitting it gives metadata only. The checkpoint comes from the observer's
supplied state; do not call document methods reentrantly from the callback.
`runAutomatic`, `runBatchImage`, profile replays and `finishEditing` throw
`Document task was cancelled.` on cancellation. A cancelled `runTask` throws
for the palette stage; segmentation, smoothing and fitting return with
`state.status` 2. Either way `snapshot({}).cancelled` becomes `true`, and
running the call again on the same document retries it.

Operation results and default progress checkpoints include dimensions,
settings, classification, parameters, stage keys, completed palettes, status,
progress, cancellation, session random state, section availability and
`fittingComplete`. They do not include raster, region or geometry arrays.

Use `snapshot({ rasters: ['working'], segmentation: 'regions' })` for pixels
and region metadata without the pixel label plane. Use `segmentation: 'full'`
to include labels. The `contours`, `smoothing` and `fitting` flags request those
sections. Unrequested bulk sections are absent; requested unavailable sections
are `null`. `snapshot({})` returns metadata only. `snapshot()` explicitly copies
all sections and can be expensive.

```js
document.runTask('smoothing', settings, {
  capture: { rasters: ['working'], segmentation: 'regions', contours: true },
  callback(event, checkpoint) {
    if (event.phase === 8 && event.substep === -1) {
      consumeCheckpoint(checkpoint);
      return false;
    }
    return true;
  },
});
```

Raster and editor pixels are `Buffer`; segmentation labels, counts and color
indices are `Int32Array`; segmentation palettes and smoothing reference
positions are `Float32Array`; byte masks are `Uint8Array`. Geometry coordinates
remain double precision. All returned buffers belong to the caller: edits,
reset and caller mutation cannot change a previously captured engine state.
This is a Node transport change, not removal of internal Rust snapshot clones.
Frozen evidence scripts and historical JavaScript component contracts use the
older full-JSON transport and are not current npm API examples.

### Edit segmentation and backgrounds

After `beginEditing()`, `edit(command)` applies `fill`, `zap` or `pencil`
commands in image coordinates with normalized BGRA colors, and
`gesture(event)` replays tool and color selection and pointer press, move and
release events. `undoEdit()`, `redoEdit()` and `revertEdits()` walk the same
history. These commands return compact `EditorSummary` metadata, including
the key, selected tool/color and undo/redo counts. Explicit getters offer:

- `editing()`: copies the editor's pixel `Buffer` and complete undo/redo records,
  or returns `null` when no editor exists.
- `editingHistory()`: returns `[undoCount, redoCount]`.
- `commitEditing()`: saves the editing checkpoint and returns `DocumentSummary`
  without vectorizing. Changed edits invalidate contours, smoothing and fitting.
- `restoreCommittedEditing()`: restores the checkpoint, or the initial editing
  history if none was committed, while preserving the selected tool. Returns
  `null` when no editor exists.
- `leaveEditing()`: ends the editing interaction without committing, refitting or
  discarding the retained working history.
- `finishEditing(settings, progress?)`: commits the edits, runs smoothing and
  fitting, and returns `DocumentSummary` plus
  `fittingCompletion: { status, didWork }`. Capture fitted geometry explicitly
  with `snapshot({ fitting: true })`.

`commitEditing()` and `leaveEditing()` require an existing editing session.
See the [editing contract](segmentation-edit-contract.json).

Background removal keeps a separate history of hidden vector regions.
`beginBackgroundEditing()`, `toggleBackground(x, y)`,
`backgroundGesture(event)`, `undoBackground()`, `redoBackground()`,
`revertBackground()` and `finishBackgroundEditing()` return compact metadata
with hidden/undo/redo counts. `background()` copies the full `hiddenRegions`,
undo and redo lists. Exports omit hidden regions. See the
[background contract](background-edit-contract.json).

### Export results

`exportBitmap(format, scale?, flatten?)` renders the fitted paths as BMP,
JPEG, PNG or TIFF bytes at `scale` times the source dimensions (default 1).
It draws new pixels rather than resizing the source. If a non-PNG format
would drop transparency, it throws an error with `code`
`BITMAP_FLATTEN_REQUIRED` unless `flatten` is `true`.

`saveVector(filename, options, progress?)` writes SVG, EPS, PDF, AI, DXF or
EMF, chosen by the filename's exact lowercase suffix. The options are
`layout` (`grouped-cutouts` by default, `cutouts` or `stacked`),
`thinStroke`, `compact` for EPS, and `curveMode` for DXF (`dense-lines` by
default, `sparse-lines` or `splines`). The optional progress callback receives
a percentage and returns `false` to cancel. The call returns
`{ status, error }`. Status 1 means an unknown suffix and 4 a failed text
open, write or close. A DXF or EMF file that cannot be opened still returns
status 0 with `error` set, so check both fields. EMF saves read the current
display's pixel and physical dimensions. On Windows, text saves let other
processes read and write the open file but not delete it, and EMF saves block
other opens. Both export calls need completed fitted geometry. See the
[bitmap export](bitmap-export-contract.json) and
[vector output](vector-output-contract.json) contracts.

### Batches

Create a batch with `session.createBatch(applicationDirectory, workingDirectory)`.
`runner.addFiles(paths)` queues files and returns the paths it skipped as
duplicates. Then call `runner.setExport(directory, format)` and
`runner.run(settings, exportOptions, overwrite, progress)`. `settings` takes
the fields described under [batch conversion](#batch-conversion), and
`exportOptions` takes the `saveVector` options. The optional overwrite
callback receives existing filenames and returns `"overwrite"`, `"skip"` or
`"cancel"`; omitting it skips existing files. Progress uses
`{ callback(event, checkpoint), capture? }`, with the same capture selection
and owned typed data as document progress. `event.state` is always a compact
`DocumentSummary`; the second argument includes requested bulk data.
Returning `false` requests cancellation, and `run` returns status `cancelled`.
The result also lists `files`, the `queue`, the processing `log` and the
`converted`, `failed` and `skipped` counts. `select`, `remove`,
`removeSelected`, `clear` and `snapshot` manage the queue. The batch snapshot's
`document` is a compact summary or `null`, not an implicit full capture.
See the [batch contract](batch-contract.json).

### Source history

`SourceHistory` keeps the three most recent source files and
`SourceDirectories` the four most recent directories, newest first. Names
match by exact string equality.
`SourceHistory.add(filename, width, height, bytes)` stores a
`WIDTHxHEIGHT, SIZE` description and thumbnail dimensions, and
`formatFileSize(bytes)` returns that size label. Neither class reads files or
persists anything; pass a stored `snapshot()` to the constructor to restore
one. See the [source-entry contract](source-entry-contract.json) and
[directory contract](source-directories-contract.json).

### Saved advanced profiles

`SavedAdvancedProfiles` holds up to 200 named advanced profiles, and a new or
loaded list places Default Advanced Settings first. `put(name, profile)` and
`select(name)` move an exact name to the front. `select` returns
`{ index, profile }`, with `-1` and `null` for an unknown name.
`rename(name, newName, force)` keeps the entry's position and needs `force` to
replace another entry. `remove(name)` returns the removed index or `-1`, and
`reset()` restores the default list. `load(path)` reads a
`{"profiles": [...]}` file and treats a missing file as the default list;
`save(path)` writes it atomically. Stored profiles drop palette colors and
make transparency colors opaque, as the original preferences do.

```js
import { SavedAdvancedProfiles, defaultAdvancedProfile } from 'openvector';

const profiles = new SavedAdvancedProfiles();
profiles.load('profiles.json');
profiles.put('Logo', defaultAdvancedProfile());
const { profile } = profiles.select('Logo');
profiles.save('profiles.json');
const replay = document.runAdvancedProfile(profile, { backup: [], customized: false });
```

`runAdvancedProfile(profile, paletteRuntime, progress?)` replays a profile
along the GUI's Fast Forward path and returns
`{ profile, paletteRuntime, state, paletteRequested, didWork }`, or `null`
when the profile's palette mode does not process. `state` is a compact
`DocumentSummary`, and `progress` uses `{ callback, capture? }`.
`applyAdvancedProfile(current, selected, batch)` returns the profile to
select, `batchProfileControls(profile)` converts one into batch settings, and
`defaultAdvancedProfile()` and `defaultBatchProfile()` return the built-in
profiles.
`applyPaletteCommand(profile, runtime, command)` applies one manual palette
edit: `count`, `replace`, `choose`, `color`, `delete`, `add`, `revert` or
`source`. See the [saved profile contract](saved-profiles-contract.json)
and [palette contract](palette-edit-contract.json).

### Engine parameters

```js
import { resolveParameters } from 'openvector';

const parameters = resolveParameters({
  mode: 'basic',
  stage: 'segmentation',
  category: 'artwork-with-blending',
  quality: 'high',
});
```

The stages are `classification`, `palette`, `segmentation`, `smoothing` and
`fitting`. Every stage except classification needs `mode`. Basic mode needs
`category` (`photograph`, `artwork-with-blending` or
`artwork-without-blending`, plus `unclassified` for the palette stage) and
`quality` (`high`, `medium` or `low`). Advanced palette needs
`colorSensitivity` and `antiAlias`. Advanced segmentation needs `complexity`,
`minimumPixels`, `artifactRejection` (`none`, `moderate` or `aggressive`) and
`clusterColors`, and its result adds `shadowing` and `clusterColors`
switches. Advanced smoothing and fitting need `smoothness`, `curveComplexity`,
`antiAlias` and `detectCorners`. Slider values are integers from 1 through 12.
Missing controls and unknown values throw; the resolver invents no
classification results or GUI defaults.

## Recent basic settings

The original application keeps a Recent Basic Settings list, separate from
named advanced profiles. One Rust collection implements it for the CLI, the
npm package and the GUI:

- It stores five profiles, newest first. Only the first four are visible, and
  the GUI and `vectorize --recent-index` replay only those.
- Each entry has a label such as `Artwork, High, Two colors`, derived from
  category, quality and palette mode. Recording a label that already exists
  moves the older entry to the front and keeps its older profile.
- Only a successful (status 0), non-batch fitting that did work records an
  entry. Automatic conversion, basic and advanced processing, profile replays
  and edit finishes all qualify. Cached, failed, cancelled and batch fittings
  record nothing.
- Storage drops palette colors and stores transparency colors as opaque.
  A collection file holds exactly `{"recentBasicSettings": [...]}`, newest
  first, and a missing file is an empty list.

Replay selects the stored profile with advanced mode off. Two-color entries
search exactly 2 colors and fixed-count entries exactly their stored count,
without the 2 through 12 fallback that advanced replay uses for a count of
zero. Unlimited-color entries skip the palette search. Entries in other
palette modes select their settings without processing.

### From the CLI

```sh
openvector vectorize logo.png --recent-file recent.json --output logo.svg
openvector recent-settings list --file recent.json
openvector recent-settings show --file recent.json --index 0
openvector vectorize next.png --recent-file recent.json --recent-index 0 --output next.svg
openvector recent-settings clear --file recent.json
```

`vectorize --recent-file PATH` reads the file before the image and writes it
after each fitting that records, so a later failure in the same command keeps
the entry. The parent directory must exist. Automatic conversion, saved
profile and recent replays, edit finishes, `{"action":"process","stage":"fitting"}`
plan steps and replaced plan sources record the settings their fitting used.
`--recent-index N` replays visible entry N, from 0 through 3, instead of
automatic mode. It needs `--recent-file` and cannot be combined with
`--profile`. A hidden fifth entry, or an entry whose palette mode does not
process, fails with a diagnostic. The recent file must differ from the output
and from `--profiles-file`.

`list` prints each entry's label, visibility and profile, and `show --index N`
prints entry N's reusable profile. Neither writes the file. `clear` prints the
removed count and writes the file only when it removed entries. `batch` has no
recent options, because batch fittings never record.

### From the npm package

`RecentBasicProfiles` wraps the same Rust collection. `entries()` returns up
to five `{ label, profile }` entries, newest first, and `visibleEntries()` the
first four. In memory, entries keep complete profiles until the list is saved
and reloaded. `recordCompletion(profile, status, didWork, batch)` applies the
recording rule and returns whether the completion was eligible, which is also
`true` for a duplicate already at the front. `status` must be an unsigned
32-bit integer. `storedProfiles()` returns the storage projection, which the
constructor accepts, and `load(path)` and `save(path)` use the collection file
format above; `save` writes atomically. `basicProfileLabel(profile)` returns
an entry label, and `applyBasicProfile(profile)` returns the controls to
select for replay. `document.runBasicProfile(profile, paletteRuntime, progress?)`
replays a profile and returns the same result shape as `runAdvancedProfile`,
or `null` when its palette mode does not process.

These calls report the fitting completion that `recordCompletion` needs.
`runAutomatic`, `runBatchImage` and `finishEditing` read theirs from the Rust
stage observers.

| Call | `status` | `didWork` |
|---|---|---|
| `runTask('fitting', …)` | `state.status` | `didWork` |
| `runAdvancedProfile`, `runBasicProfile` | `state.status` | `didWork.fitting` |
| `runAutomatic`, `runBatchImage` | `fitting.status` | `fitting.didWork` |
| `finishEditing` | `fittingCompletion.status` | `fittingCompletion.didWork` |

Record the profile your application selected for that run. After
`runAutomatic`, the CLI updates its selected profile with the result's
`settings` and `recommendedBackgroundMode`, and sets `colorCount` and
`paletteColorCount` to `paletteCount` when it is not `null`. Pass
`batch: true` for `runBatchImage` and `BatchRunner` work, which Rust then
declines.

```js
import { readFileSync } from 'node:fs';
import { CoreSession, RecentBasicProfiles, applyBasicProfile } from 'openvector';

const recent = new RecentBasicProfiles();
recent.load('recent.json');
const record = (profile, status, didWork) => {
  if (recent.recordCompletion(profile, status, didWork, false)) recent.save('recent.json');
};

const document = new CoreSession().createDocument();
document.setSource(readFileSync('logo.png'), 'logo.png');
const [entry] = recent.visibleEntries();
const replay = entry && document.runBasicProfile(applyBasicProfile(entry.profile), { backup: [], customized: false });
if (replay) {
  record(replay.profile, replay.state.status, replay.didWork.fitting);
  document.beginEditing();
  document.edit({ tool: 'fill', x: 8, y: 8, color: [0, 0, 1, 1] });
  const { status, didWork } = document.finishEditing(replay.profile.settings).fittingCompletion;
  record(replay.profile, status, didWork);
}
```

### In the GUI

The Mode page lists the first four labels in a Recent Basic Settings group,
collapsed at startup and without a Clear control. Choosing a label replays
it. Success opens Review, or Export when Skip review is on; failure or Cancel
leaves the replayed settings selected on Mode. An application reset clears
the list. The GUI stores it under `recentBasicSettings` in its preferences
file.

### Evidence limits

A retained 30-action native history covers duplicate labels, the hidden fifth
entry and replay navigation, alongside the static analysis and persistence
cross-check in the
[recent basic settings contract](recent-basic-settings-contract.json).
The current GUI and Rust engine match that history's captured settings,
palettes, geometry and random state, including the exact exported AI file.
This finite history does not establish every possible profile or palette
mode. The original's palette handling for a fixed count of zero remains
untraced; replay passes the count to the palette task unchanged, and its error
reaches the caller. Rust writes the list as soon as an entry records, which
does not establish the original's teardown or crash timing.

## Use the CLI

```sh
node bin/openvector.mjs --help
node bin/openvector.mjs vectorize logo.png --output logo.svg
node bin/openvector.mjs batch first.png second.jpg --format svg --output-dir vectors
```

An installed package exposes the same commands as `openvector`, which the
examples below use. The launcher passes arguments and inherited standard
input/output/error directly to the compiled executable. On POSIX it replaces
itself with the Rust process, preserving the PID and signal behavior. On
Windows the child shares the console; the launcher lets Rust handle Ctrl-C and
Ctrl-Break without sending a second, forceful termination signal. Windows
process-kill APIs are not console interrupts and cannot request graceful
cancellation.

Every command has `--help`, and `openvector --version` prints the package
version. The commands are:

- `vectorize` converts one image.
- `batch` converts several images with shared settings.
- `profiles` manages saved advanced settings.
- `recent-settings` inspects or clears [recent basic settings](#recent-basic-settings).
- `settings`, `fill`, `assign-palette`, `generate-palette`, `refine-palette`,
  `analyze-image`, `segment`, `trace-contours`, `smooth-contours`,
  `fit-contours` and the `export-*` commands run single engine stages on
  JSON input.

### Convert one image

```sh
openvector vectorize logo.png > logo.svg
openvector vectorize logo.bmp --output logo.pdf
openvector vectorize logo.png --output traced.png --scale 2
openvector vectorize logo.png --format jpg --flatten > traced.jpg
openvector vectorize - --filename logo.png < logo.png
```

Input stays full-size unless you pass `--shrink-source`. That option uses the
shared engine's recommended reduction above 1.6 megapixels before processing,
including replacement images in an edit plan. It does not change source files.
Unlike `--scale`, it changes processing dimensions rather than bitmap export size.

`vectorize` reads PNG, BMP, JPEG, GIF or TIFF and runs the automatic workflow.
It writes SVG, EPS, PDF, AI, DXF or EMF vectors, or BMP, JPEG, PNG or TIFF
bitmaps rendered from the fitted paths. The output suffix selects the format,
`--format` overrides it, and standard output defaults to SVG. Vector output
accepts `--layout` (`grouped-cutouts` by default, `cutouts` or `stacked`),
`--thin-stroke`, `--compact` for EPS and `--dxf-mode` (`dense-lines` by
default, `sparse-lines` or `splines`). Bitmap output accepts `--scale` and
`--flatten`, which acknowledges that a non-PNG format drops transparency; the
CLI never prompts. EMF output uses the current display's pixel and physical
dimensions. Without a display, use `export-emf` with explicit
`referenceDevice` metrics.

Binary standard input needs `--filename`, because the source name affects
processing. Existing outputs need `--force`, and the output must differ from
the input and from any profile or recent settings file. The CLI writes the
output only after conversion succeeds. Errors go to standard error with exit
status 1.

Decoder coverage and known decoder gaps are recorded in the
[image input](image-input-contract.json),
[BMP](bmp-input-contract.json), [GIF](gif-input-contract.json),
[JPEG](jpeg-input-contract.json) and [TIFF](tiff-input-contract.json)
contracts. Seven 16-bit Adam7 PNG comparisons still fail; see the parity
matrix's known failing cases.

### Edit plans

`--edits PATH` applies a JSON array of commands after automatic processing:

```sh
openvector vectorize logo.png --edits edits.json --output edited.svg
```

```json
[
  {"tool":"fill","x":8,"y":8,"color":[0,0,1,1]},
  {"tool":"pencil","from":[1.2,1.3],"to":[13.4,12.5],"color":[0,0,0,1]},
  {"action":"undo"},
  {"action":"redo"}
]
```

Tools are `fill`, `zap` and `pencil`, with normalized BGRA colors and image
coordinates. `{"gesture": event}` entries replay tool selection, color
selection and pointer press, move and release events. The actions are `undo`,
`redo`, `revert`, `begin` to re-enter the retained editor, `commit` to commit
pixels without refitting, `restore-committed` to restore the last committed
editing checkpoint, and `leave` to select Pan without changing pixels or
history. `finish` refits; `process` runs one `stage` (`classification`, `palette`
with `lower` and `upper`, `segmentation`, `smoothing` or `fitting`) with the
selected controls. Remaining changed edits are refitted when the plan ends.
`{"settings": {...}}` changes the selected processing controls for later steps.
`{"source": "next.png"}` opens and automatically processes another image,
resolved from the plan's directory.

Background commands hide and restore vector regions with their own history:

```json
[
  {"background":{"action":"begin"}},
  {"background":{"action":"toggle","point":[1.5,2]}},
  {"background":{"action":"undo"}},
  {"background":{"action":"redo"}},
  {"background":{"action":"finish"}}
]
```

Background actions also include `revert`, and `gesture` with a nonempty
`events` array. At the end of a plan, uncommitted pixel edits refit with the
last committed settings. A plan that leaves no fitted vectors fails without
writing output. See the [editing contract](segmentation-edit-contract.json)
and [background contract](background-edit-contract.json).

### Saved profiles

```sh
openvector profiles list --file profiles.json
openvector profiles show --file profiles.json --name "Default Advanced Settings"
openvector profiles save --file profiles.json --name Logo --input profile.json
openvector profiles select --file profiles.json --name Logo
openvector profiles rename --file profiles.json --name Logo --new-name Artwork
openvector vectorize logo.png --profile Artwork --profiles-file profiles.json
openvector batch first.png second.png --profile Artwork --profiles-file profiles.json
openvector profiles delete --file profiles.json --name Artwork
```

`show` prints a reusable profile record. `save` accepts a JSON file or
`--input -` for standard input; replacing an exact name requires `--force`.
`list` and `show` never write the storage file. Every command starts a fresh
session, so Default Advanced Settings appears first on read. Rename keeps the
source settings and position, and a collision requires `--force`. Empty or
unchanged new names write nothing. Delete removes the exact named entry
without a prompt. The storage file's parent directory must exist, and the
`profiles` commands never run a conversion.

`vectorize` and `batch` select a saved profile when given both
`--profile NAME` and `--profiles-file PATH`. Selection moves the name to the
front and persists that order once the image loads. `vectorize` then follows
the GUI's Fast Forward path, skipping classification and using the stored
palette metadata. A profile with auto-detect palette mode does not process,
and the command fails without output. `batch` applies the saved advanced
controls after `--settings`, keeping that file's transparency and basic
choices. No implicit GUI storage file is used. See the
[saved profile contract](saved-profiles-contract.json).

### Batch conversion

```sh
openvector batch first.png second.jpg --format svg --output-dir vectors
openvector batch first.png second.png --settings batch.json --overwrite replace
```

The export folder must already exist. Without `--output-dir`, batch uses the
folder of the last source. The formats are SVG, EPS, PDF, AI and DXF, with AI
as the default. As in the original, batch defaults to automatic category,
quality and palette detection and flattens to white. Existing outputs are
skipped unless `--overwrite replace` is selected. With `--overwrite cancel`,
any existing output stops the batch at preflight with exit status 130.
The command never prompts. It prints a JSON summary with the processing log,
continues after load and save failures, reports them on standard error, and
exits 1 if any file fails.

Ctrl-C requests cancellation at the next numerical-stage checkpoint or task
boundary and exits 130 with a cancelled JSON summary. A second Ctrl-C forces
termination without a summary. Completed files remain; an active write can be
incomplete after forced termination. Windows Ctrl-Break uses the same
controls.

`--settings` takes a JSON object. The basic fields are `category`
(0 photograph, 1 blended artwork, 2 flat artwork, 3 auto), `quality` (0 high,
1 medium, 2 low, 3 auto), `paletteMode` (1 fixed, 2 flexible, 3 auto),
`colorCount` (0 for automatic, or 2 through 12) and `backgroundMode`
(0 preserve, 1 white, 3 auto). Their defaults are 3, 3, 3, 0 and 1. For two
colors, use `paletteMode: 1` and `colorCount: 2`. Set `advanced: true` to skip
classification and supply processing controls:

```json
{"advanced": true, "paletteMode": 1, "colorCount": 24, "complexity": 5, "smoothness": 6}
```

Advanced `colorCount` runs from 2 through 24 and defaults to 2. Only fixed
mode searches for a palette. Advanced `paletteMode: 0` with a normalized BGRA
`palette` array reuses those colors without palette generation; the GUI's
batch menu does not offer that mode. The other advanced fields are
`sensitivity`, `antiAlias`, `complexity`, `minimumPixels`,
`artifactRejection`, `clusterColors`, `smoothness`, `detectCorners`,
`curveComplexity` and `smoothingAntiAlias`. Unknown fields fail. See the
[batch contract](batch-contract.json).

### Engine stage commands

These commands read one JSON file, or standard input for `-`. Export commands
write the vector file to standard output, and the others print JSON. Each
invocation starts a fresh session. Errors go to standard error with exit
status 1 and no partial output.

| Command | Input | Output |
|---|---|---|
| `settings` | `resolveParameters` options | Numeric parameters |
| `fill MAP SEED REPLACEMENT` | `{ width, height, segments }` | The edited map |
| `assign-palette` | `bgra` bytes and a normalized BGRA `palette` | One palette index per pixel |
| `generate-palette` | `bgra`, `colorCount`, `bins` (8 or 16), `connected`, `antiAlias` | `palette`, `indices`, `weights`, `passes` |
| `refine-palette` | `bgra`, `palette`, `antiAlias` | `palette`, `indices`, `weights`, `passes` |
| `analyze-image` | Decoded `width`, `height`, `bgra`, `hasAlpha`, `extension`, preprocessing `mode` and `amount`, `parameters`, `paletteControls` | Classification, working size, palette candidates, random state |
| `segment` | Prepared `width`, `height`, `bgra`, `settings`, optional `palette` | `indices`, `counts`, `palette`, `colorIndices` |
| `trace-contours` | The same input as `segment` | Shared contour geometry |
| `smooth-contours` | `width`, `height`, `bgra`, `indices`, `palette`, raw `geometry`, `settings` | `geometry`, `penaltyCounts` |
| `fit-contours` | Smoothed `geometry` and `settings` | Fitted geometry |
| `export-svg`, `export-eps`, `export-pdf`, `export-ai`, `export-dxf`, `export-emf` | Fitted `geometry`, source `width` and `height`, export options | The file bytes |

`settings` objects inside stage input use the `resolveParameters` options
without `stage`; the command supplies the stage. Weighted palette input
(`antiAlias: true`) also needs `width` and `height`. When the resolved
segmentation parameters enable `shadowing`, `segment` and `trace-contours`
also need a top-level integer `shadowControl`. Export input accepts
`hiddenRegions`, `layout` (`stacked` by default here), `thinStroke` and an
optional `outputWidth` and `outputHeight` pair. EPS adds `compact` and
`creationDate`, DXF requires the segmentation `palette` and accepts
`curveMode`, and EMF requires `referenceDevice` with `pixels`, `millimeters`
and `micrometers` pairs.

For example, this `settings.json` prints advanced segmentation parameters:

```json
{"mode": "advanced", "stage": "segmentation", "complexity": 6, "minimumPixels": 0, "artifactRejection": "moderate", "clusterColors": true}
```

```sh
openvector settings settings.json
```

To edit a segmentation map, save `{"width": 2, "height": 2, "segments": [0, 1, 1, 0]}`
as `map.json` and run `openvector fill map.json 0 7`. The seed is a row-major
pixel index, and the replacement identifier must differ from the seed's
current one. The result has segments `[7, 1, 1, 0]`, because diagonal contact
does not join regions.


### SVG dimensions and display units

SVG export writes dimensions in points, with 72 points per inch, to preserve the reference format's physical print size. A 48 by 48 source therefore produces 48 pt dimensions; at 96 DPI, browsers and many editors render that as 64 by 64 CSS pixels. The 1× bitmap export instead uses the source pixel dimensions. To get 48 CSS pixels on the web, set the SVG's CSS width and height to 48 px, or adjust the dimensions in an editor; this changes its displayed size without changing the viewBox.
