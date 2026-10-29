# Review Result engine and API

`GuiEngine.review` implements the native review policy through existing palette,
processing and editor owners. It preserves selected controls instead of invoking
Fully Automatic and resetting them. `/api/review` exposes the same command through
the worker and the existing single-image ownership guard.

The new native observations identify page 18 as Export Result and page 19 as
Review Result. Review contains Done Reviewing, three detail levels, two color
modes, background removal, segmentation editing, hand-picked settings and the
saved skip checkbox. The native handler `0x432750` accepts choices 0 through 8.
Its complete assembly reads only the second argument; the sender pointer is
unused. The observer therefore passes null to the model method and drives no
native control.

The 22 action workflows cover opaque BMP and translucent PNG sources. They
repeat quality changes, repeat the current quality, and switch from flexible to
custom colors. All 44 fresh native processes repeat byte-for-byte. The 30 full
document captures include original/prepared pixels, cache state, numerical
parameters and geometry. Independent tests also compare selected profiles,
palette backup state and destination pages after every action.

Six additional native processes observe captions and navigation. The hidden
window cannot advance through Next or Back. The existing private-desktop
observation routine establishes Qt visibility without showing on the input
desktop; Next then reaches export and Back returns to Review. An initial guessed
procedure address was rejected by REA. The Qt method dispatch table supplied the
correct handler address; the rejected request remains in the evidence.

Code simplification ran the shipped reuse, quality and efficiency rubrics inline,
with zero independent reviewer contexts. Applied findings: reuse 0, quality 0,
efficiency 0; skipped 0. The implementation already reuses `#findPalettes`,
`applyPaletteCommand`, `process`, `edit` and `background`. No unused production
elements were identified. This JavaScript/JSX project has no lint or typecheck
scripts. Native expected records and existing assertions were preserved.

The isolated build passes. Its full GUI suite has 411 passes, one existing
batch-transparency failure and zero skipped tests. All 24 new tests pass.
The workspace build is preserved. The earlier two full-core failures were
not rerun or resolved by this change.

The shared recorder previously reported no native widget setters even for its
observation-only visibility setup. That metadata now reports the setter
accurately. Twenty-three current proof records were corrected without changing
raw captures, requests, expectations or fixture bytes. The recorded scope is
WA_DontShowOnScreen and window showing on the private desktop, with no native
checkbox values, button input or dialog answers supplied by the observer.

The GUI controls and saved skip checkbox are not wired yet. These observations
do not establish review behavior with customized palette/segmentation histories,
complete effective group visibility, all source categories, active-job shutdown,
or complete window parity. Those gaps remain part of the active goal. This work
does not change core algorithms, the CLI, package exports, the original installed
application, existing preview channels or the workspace GUI build.
