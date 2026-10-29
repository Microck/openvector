# Auto-update preference review

The three checked choices now survive clean host restarts and workspace changes.
The timer owner remains in `AutoUpdate`; the worker stores its checked projection
for the preference writer. Pending work, page enablement and deadlines are never
serialized. The existing native dispatch and re-entry assertions are unchanged.

The native MainWindow constructor calls reader `0x44e850`. That reader restores
the three `VmQt/auto_*` fields after constructing the checkboxes, using true as
each missing-value default. Nine startup cases cover cold defaults and all eight
Boolean combinations. Each case repeats in a fresh process. The recorder seeds
only its owned private registry namespace, observes Qt getters and removes the
namespace afterward. All 18 accepted raw responses reparse and repeat exactly.
The original installation, settings and input desktop remain unchanged.

REA also recovered the window destructor `0x44f0f0`, its writer `0x44c7d0` and
the startup reader. The destructor calls the writer before freeing its owned
members. The writer saves the checkbox fields before the model preferences.
This is static evidence. The observer does not invoke either window routine,
drive a control, answer a dialog or claim active-job shutdown parity.

The new read-only label observer identifies Recent Folders, Recent Images,
Recent Basic Settings, Saved Advanced Settings, Save Settings, Quick Save and
Bitmap Export groups. Its two repeated workflows cover eight processes and
nine snapshots. Labels do not prove disclosure persistence or full window reset.
No disclosure UI was changed in this work.

The storage boundary requires exactly four root fields, including `autoUpdate`.
HTTP mutations validate the entire three-Boolean projection before replacement.
Older local shapes fail with explicit recovery instructions. There is no second
format, migration shim or hidden fallback. Existing preference test harnesses now
supply the new canonical field; native expected records and assertions remain
unchanged. Files are written only on clean idle host shutdown.

The isolated GUI build passes. Its full suite has 387 passes, one existing
batch-transparency failure, and zero skipped tests. All 11 new tests pass. The
browser verifies all three choices before and after a batch round trip and clean
host restart. Each manual processing stage also works with its automatic box
off. Both verification hosts closed normally. One earlier host was started
before the suite finished, closed without a browser interaction, and excluded
from browser evidence. Its runtime records remain available.

Code simplification used the shipped reuse, quality and efficiency rubrics inline
with zero independent reviewer contexts. Applied findings: reuse 0, quality 0,
efficiency 0; skipped 0. No new unused production elements were identified.
There are no configured lint or typecheck scripts in this JavaScript/JSX project.

The workspace build and user preview channels were preserved. Core source,
CLI and npm package files are unchanged from the preceding accepted archive.
The two previously recorded full-core failures were not retested or resolved by
this GUI change. Skip Troubleshooting, disclosure owners, Quick Save, Bitmap
Export, Drag Result, full window reset, active-job shutdown, final packaging,
GitHub publication and the Rust/GPUI port remain unfinished.
