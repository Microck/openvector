# Editing while inspecting another layer

The selected editing tool must receive admitted pointer events independently
of the displayed Bitmap, Segments or Vector layer. Selecting or temporarily
inspecting a layer changes rendering, while the tool still edits its model.
The original pointer handlers dispatch on input mode. Their sampling helper
0x445880 takes an explicit source/segmentation argument and always maps through
the source graphics item. Pencil supplies the segmentation argument. Model
record dispatch 0x431500 has no display-layer gate. Existing captured B/V
transitions preserve the editing tool.

The first wrong effect was in React: pointer press, initial held Pencil move
and picker release required a particular shown layer before enqueuing input.
These checks stopped valid requests before the HTTP worker could see them.
The current pre-change browser reproduces all three failures on Bitmap:
Pencil records nothing, Picker never restores its tool, and Erase hides nothing.
No native synthetic pointer/key input, widget setter or dialog was used for
this investigation. Static evidence does not establish complete native input
or layer-transition parity.

Viewer now checks admission against editing state, processing/confirmations
and the mouse button. The existing ordered queue, capture, world mapping,
selected tool and rendering layer remain separate owners. No new runtime
module, wire field, compatibility path or fallback was introduced. The removed
conditions were unsupported behavior rules, not trust-boundary validation.

The ce-simplify-code reuse, quality and efficiency reviews ran inline with
zero independent reviewers under the explicit no-delegation rule. No further
finding justified a code change: applied reuse 0, quality 0, efficiency 0;
skipped 0. The remaining shown-layer value owns rendering, and backgroundEditing
still controls the Pencil-entry path. No unreachable product code was found.
The shell setup could not launch its WSL runtime; the same context script
completed through Windows Node with CE_CONTEXT_END. No lint or typecheck script
is configured in this JavaScript project.

The consolidate-test-suites invariant is event admission independent of the
rendered layer. Its owning layer is end-to-end because the defect lives in
React DOM handlers and a unit state snapshot cannot prove those handlers ran.
The existing viewer-input unit suite remains unchanged and passes 15 tests.
The browser evidence follows the existing evidence/verify-viewer-input-browser
pattern. A separate bounded probe is justified because no canonical browser
runner mounts these React handlers; putting a Chrome/CLI dependency in the
normal Node suite would obscure its existing ownership. The deterministic
probe covers a durable user action and reproduces the pre-change failure.
No duplicate state-only test or new native fixture was added.

Probe dispositions: the minimal browser verifier, before/after observations,
network evidence, screenshots and extracted original instruction evidence are
PROMOTE. MERGE and DROP: none. There is no temporary product instrumentation,
new native helper or task-created compiler object. Earlier probes and captures
are preserved.

Thirteen post-change browser observations verify Pencil press/move on Bitmap,
a held Pencil move entering without a viewer press, edits during B inspection,
picker release on Bitmap and Vector, B/V restoration, and background removal
while Bitmap is shown. Picker samples the red edited segmentation where the
original source pixel is white; opacity returns from one half to one.
Persistent controls and mouse samples use browser input through agent-browser.
Held B/V keys use declared DOM KeyboardEvent producers because the installed
CLI lacks a held-key command. This is not a trusted held-key or native pointer
capture. All recorded edit/background HTTP responses return 200 and the browser
error list is empty. Both owned browsers and hosts close normally.

The isolated production build passes. The full GUI run has 657 tests, 656
passing and the same batch-reset failure; zero tests are skipped or weakened.
The prior core result is reused only after all package sources and core tests
are shown unchanged: 866 tests, 864 passing, with the same encoded-PNG and
edited-alpha failures. The existing 123-file private package remains identical;
no new archive or npm install was needed for a GUI-only change.

The initial Zap investigation found an unchecked neighbor-set begin read in
the original handler. It supplies no stable empty-neighbor result, so the
existing explicit diagnostic is retained. No sentinel or heap bytes are copied
into the product to manufacture parity. Full application parity, publication
and the Rust/GPUI port remain unfinished.
