# Review Result GUI and skip preference

The GUI now exposes the verified review actions through the existing worker API.
Quality and color-mode buttons show the selected controls. Fixed/two-color modes
show the current palette and Change action; flexible color mode hides that strip.
Remove Background, Edit Result and Hand-pick Settings reuse their existing owners.
Done Reviewing and Next reach Export. Basic Export includes Improve Result.

Read-only Qt observations distinguish effective visibility from a child's hidden
flag. They confirm that Process as Artwork stays hidden for all three source
categories. Native page 15 is Remove Background, 17 is advanced Export, 18 is basic
Export and 19 is Review. The earlier contract's page-17 description was corrected
from actual widget captions rather than carried into the implementation.

The visibility and Back corpora contain 11 workflows, 22 native processes and
64 widget snapshots. Both skip values are seeded before startup in the owned
private QSettings namespace. Model commands provide navigation; the observer
does not click a native button, set a checkbox value or answer a dialog. It uses
the existing WA_DontShowOnScreen/private-desktop visibility preparation. Every
fresh response repeats exactly and reparses. The prior 26 native helpers and
the original installation/settings remain unchanged.

`review-state.mjs` owns the checked projection and the result-page routing policy.
The storage boundary now requires exactly five root fields. The worker restores
Skip once, exposes the shared Boolean through `/api/reviewPreferences`, preserves
it during model reset and writes it on clean idle shutdown. Invalid mutations
leave the current choice intact. Invalid/older local files fail with an explicit
recovery path. Existing test harnesses supply the new field; their native
expectations and assertions are preserved.

The browser verifies Medium, both color modes, palette processing, both editors,
hand-picked settings, Done, Improve, Back routes, the background-completion
exception, restart and a batch round trip. An initial Export shortcut retained
the bitmap layer after Hand-pick Settings. The shortcut now selects the vector;
that branch was retested in the restarted host. The initial build and test logs
remain available. A final spacing pass places all eleven Review controls inside
the panel at 1280x720. It changes no global styling or motion.

The final isolated build passes. Its full GUI suite has 423 passes, one existing
batch-transparency failure and zero skips. All 12 new tests pass. The browser has
28 recorded states and no console warnings or errors. Both owned hosts close
normally. The workspace build and existing user previews are preserved.

The Octen Design reference was ImagiTool's photo-adjustment editor illustration.
Only compact labeled controls beside the image informed the layout. OpenVector
keeps its black background, white text and existing shadcn/Lucide components.
The reference's marketing layout, colors and text were not copied.

Code simplification used the shipped reuse, quality and efficiency rubrics inline
with zero independent reviewer contexts. Applied: reuse 0, quality 0, efficiency 1
(compute each palette swatch once); one small extraction was skipped because it
would add an abstraction without reducing ownership complexity. No unused
production elements were identified. No lint/typecheck scripts are configured.

Other window owners, customized review histories, full reset and shutdown parity,
the existing GUI failure and two earlier core failures remain unfinished. Core
algorithms, CLI/package files and original application files were not changed.
This checkpoint does not claim completion, publication or the later Rust/GPUI port.
