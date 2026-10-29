# OpenVector GPUI implementation spec

Paths are relative to `vector-magic-re/` unless they are URLs. Line numbers read on 2026-10-20.

## 1. Vector Magic GUI inventory (from the repo)

Behavior source of truth: `docs/gui-contract.json:1-80` (reference vmde.exe SHA `c491656b...`; "preserve workflow, control grouping and feature set"; background `#000000`, white text; no cards/pills; no looping animation; viewer readout not selectable). Port requirement: `docs/rust-port-contract.json:2-25` (recreate the shadcn preview in GPUI; JS stays a development reference, never a fallback; pin a verified revision). Existing Rust workspace: `rust/Cargo.toml` (members `crates/openvector`, `crates/openvector-cli`, edition 2024, MIT).

### 1.1 Application shell
| Element | Source |
|---|---|
| Workspace modes loading / single / batch; load error + Reload | `gui/src/app.jsx:57-107` (error view 89-90) |
| Startup state reads: batch, profiles, sources, autoUpdatePreferences, reviewPreferences, quickSave, bitmapSave | `gui/src/app.jsx:80` |
| Header: logo + "openvector", file name + W x H, Open, Batch, Export (needs vector), About | `gui/src/app.jsx:701-716` |
| Body: Viewer left, Wizard right | `gui/src/app.jsx:717-740` |
| Footer: local indicator, page title or status, progress + Cancel | `gui/src/app.jsx:742-746` |
| Drag and drop open with overlay | `gui/src/app.jsx:697-700,747` |
| Overwrite / flatten confirmation (modal, no outside dismiss, focus restore) | `gui/src/app.jsx:25-55`, mounted 741 |

### 1.2 Global keyboard
`gui/src/app.jsx:676-694`: Ctrl/Cmd+Z undo, Ctrl+Shift+Z or Ctrl+Y redo (editing/background, not in text fields); Alt+R Start; PageUp, Alt+B, Alt+Left Back; PageDown, Alt+N, Alt+Right, Enter Next; all key press/release also forwarded to viewer input.

### 1.3 Wizard pages
Titles `gui/src/wizard.jsx:12-14`. Bodies `gui/src/wizard.jsx:208-345`. Next graph `gui/src/app.jsx:483-511`. Review/back policy `gui/review-state.mjs`, `docs/review-ui-contract.json:20-23`.

| id | Title | Controls | Source |
|---|---|---|---|
| load | Load image | Browse, recent directories (reopen, pick, reveal) | `wizard.jsx:213-232` |
| mode | Wizard mode | Fully automatic, Basic, Advanced | `wizard.jsx:233-239` |
| type | Image type | Photograph, Artwork with blended edges, Artwork without blending | `wizard.jsx:240-244` |
| transparency | Transparency | Leave transparent, white, custom color; only when source has alpha (`app.jsx:487`) | `wizard.jsx:245-251` |
| quality | Input quality | High, Medium, Low | `wizard.jsx:252-255` |
| colors | Color mode | Two colors, Fixed palette, Flexible palette | `wizard.jsx:256-261` |
| palette | Palette selection | advanced color mode, count 2..24, swatches, analyze | `wizard.jsx:165-206,262` |
| segmentation | Image segmentation | Complexity 1..12, Minimum pixels per region, more | `wizard.jsx:263-272` |
| smoothing | Path smoothing | Smoothness, Curve complexity, Detect sharp corners | `wizard.jsx:304-310` |
| editing | Edit segmentation | Pan, Pencil, Fill, Zap, Pick color; undo, redo, revert | `wizard.jsx:273-290`; `docs/segmentation-edit-contract.json` |
| background | Remove background | Pan, Erase | `wizard.jsx:291-303`; `docs/background-edit-contract.json` |
| review | Review result | Done reviewing; High/Medium/Low; Unlimited/Custom colors; Remove Background, Edit Result, Hand-pick Settings; Skip this page next time | `wizard.jsx:311-338`; `docs/review-ui-contract.json:20` |
| export | Export result | Format AI/DXF/EMF/EPS/PDF/SVG; Stacked / Cut out / Grouped cutouts; thin outline; compact EPS; Quick Save; Bitmap export | `wizard.jsx:339-360`; `docs/export-preferences-contract.json` |

Frame: "Basic wizard"/"Advanced wizard" + title `wizard.jsx:368-369`; advanced Auto-Update Palette/Segmentation/Vectorization `wizard.jsx:371-372` (`docs/auto-update-contract.json`, `gui/auto-update.mjs`); status line (`role=alert` on error), Start/Reset, Back/Revert, Next `wizard.jsx:375-379`.

### 1.4 Advanced panels and shared controls
`MenuSelect` `wizard.jsx:16-21`; `NumericControl` slider 1..12 `wizard.jsx:101-105`; `CheckControl` `wizard.jsx:107-109`; `ColorCount` clamp 2..24 `wizard.jsx:165-177`; `PaletteControls` `wizard.jsx:179-206` (`docs/palette-edit-contract.json`, `gui/palette-selection.mjs`).

### 1.5 Viewer
`gui/src/viewer.jsx:50-180`.
- Tool rail: Pan up/left/right/down `viewer.jsx:126-131`; Zoom in, vertical zoom slider 0..300, Zoom out, 100%, Fit `viewer.jsx:134-141`; layers Bitmap (hold B), Segmentation, Vector (hold V), Paths checkbox `viewer.jsx:144-150`.
- Viewport: focusable, pointer-anchored wheel zoom `viewer.jsx:154-157`; scrollbars `viewer.jsx:27-48`; dashed removed-region outlines `viewer.jsx:171-173`.
- Zoom math `docs/viewer-zoom-contract.json`: slider 0..300, step 1, page 10 (`:7-12`); scale = exp(lerp(ln 0.2, ln 24, s/300)) (`:17`); wheel slider delta = trunc(delta/12), pointer anchored (`:39-42`); Fit = min(available/image), available = widget minus 17 px scrollbar extent, no margin, recenter (`:50-51`); 100% = exact scale 1. Code `gui/viewer-zoom.mjs`, `gui/viewer-zoom-values.mjs`, `gui/viewer-geometry.mjs`.
- Key state `docs/viewer-input-contract.json:20-26` (`gui/viewer-input.mjs`): B/V hold sets Current layer if enabled, release restores Base when neither held; Shift with non-Pan tool = temporary Pan retaining tool; retained-restore quirk; B/V honored with Ctrl/Alt/Meta and on repeat; suppressed in mode 10 and batch. Primary/Base/Current layers distinct.
- Pointer chords `docs/viewer-input-contract.json:41-52`: bits L=1, R=2, M=4; tools use left held bit; Pan starts on left press, ends on any release (Qt 4.4.3 ScrollHandDrag).

### 1.6 Batch window
`gui/src/batch.jsx:13-120`; `docs/batch-contract.json:213-252`.
- Pages files, settings, progress, summary (`batch-contract.json:215-220`).
- Settings `batch.jsx:27-66`: Advanced controls; basic Image type / Image quality (Auto-detect=3); Color mode; Number of colors (basic Auto + 2..12, advanced 2..24); Transparency; advanced "Processing controls" (Color sensitivity, blended edges, Complexity, Minimum pixels, Artifact rejection, Cluster colors, Smoothness, Curve complexity, Detect sharp corners, Anti-alias contours); Export folder + browse; Format AI/SVG/EPS/PDF/DXF; Shape mode; Thin outline; Compact EPS (EPS only).
- Defaults `batch-contract.json:20`; queue identity `:58-66`; cancellation `:181-209,248`; commands `:246`; during a job only reads, overwrite decisions, cancel (`:247`); thumbnails `batch.jsx:13-25`, `:251`.

### 1.7 Profiles, preferences, Quick Save, resets
- Saved settings: Play, Fast Forward, Save (exact name, overwrite confirm), rename, delete: `wizard.jsx:111-157`; `docs/saved-profiles-contract.json`, `docs/profile-*-contract.json`; `gui/profile-controller.mjs`, `gui/profile-name.mjs`.
- Preferences: `docs/processing-preferences-contract.json`, `docs/review-ui-contract.json`, `docs/export-preferences-contract.json`, `docs/window-export-preferences-contract.json`, `docs/auto-preferences-contract.json`; `gui/processing-preferences.mjs`.
- Quick Save (collapsible, separate basic/advanced choice, folder + name + Save): `wizard.jsx:30-59,349-354`; `docs/quick-save-contract.json` plus `quick-save-direct`, `-cancellation`, `-batch` contracts; `gui/quick-save.mjs`, `gui/quick-save-file.mjs`.
- Bitmap export (size fields, scale buttons, Use view scale, flatten acknowledgement): `wizard.jsx:61-99`; `docs/bitmap-gui-contract.json`, `docs/bitmap-size-controls-contract.json`; `gui/bitmap-size.mjs`.
- Recent directories: `docs/source-directories-contract.json`, `gui/source-controller.mjs`.
- Resets: `docs/application-reset-contract.json`, `processing-reset`, `factory-reset`, `window-reset` contracts.

### 1.8 Dialogs and errors
| Dialog | Source |
|---|---|
| Overwrite file/directory, flatten acknowledgement | `app.jsx:25-55` |
| Delete settings, Overwrite Settings, Change Settings Label | `wizard.jsx:140-155` |
| About | `app.jsx:711-715` |
| Batch overwrite decision | `batch-contract.json:59,248` |

Errors: wizard status `role=alert` `wizard.jsx:376`; failed commands set `{kind:'error'}` and reapply carried `state` `app.jsx:232-245`; processing poll/cancel errors `app.jsx:157-166`; workspace load error `app.jsx:89-90`; Cancelled footer `app.jsx:742`.

## 2. Zeron design language, adapted to true black

Repo: https://github.com/zeronsh/zeron (main, MIT, workspace version 0.2.114). Rust GPUI desktop app; tree fetched via gitquarry.

### 2.1 Structure
- `Cargo.toml`: workspace of `crates/*` + `apps/zeron`; GPUI is a fork `zeronsh/zui` rev `361ff20b919fdfb80199ee6d9aed725300cb7d54` (extracted from Zed, adds BackdropBlur, EdgeFade, rounded clip); `gpui_platform` features `wayland, x11, font-kit, runtime_shaders`; `gpui-base` from `zeronsh/gpui-component` rev `701ca97a`. Dev profile: deps at opt-level 2 ("gpui is slow unoptimized").
- `crates/theme/src/{lib,builtins,library,vscode}.rs`: source-neutral `ThemeVariant` of semantic roles (`docs/theme-system.md`).
- `crates/ui/src/`: `theme.rs` (layout tokens), `typography.rs`, `motion.rs`, `icons.rs`, `popover.rs`, `pickers.rs`, `settings.rs`, `toast.rs`, `notice.rs`, `image_viewer.rs`, `rail.rs`, `shell.rs`, `surface_chrome.rs`, `loaders.rs`.
- `docs/adr/0001-custom-themes-compile-to-resolved-families.md`, `0002-separate-surface-preference-from-theme-selection.md`.

### 2.2 Tokens
Layout (`crates/ui/src/theme.rs:815-846`): HEADER_HEIGHT 44, TITLEBAR_HEIGHT 38 (+4 top pad), STATUS_STRIP_HEIGHT 24, PANEL_RADIUS 10, CONTROL_RADIUS 6, BUBBLE_RADIUS 16, SPACE_XS/SM/MD/LG 4/8/12/16, TEXT_STACK_GAP 1. Scrim 0.60 (`theme.rs:1737`). Hairline scale 1.35 light (`theme.rs:374`).

Color derivation (`crates/theme/src/builtins.rs:97-143`): hover = white 0.11 alpha (dark), active = accent 0.18, border = white 0.10, border_strong = white 0.18, muted ensured 4.5:1 against background, input = raised at 0.72, cursor = text 0.40, solid #ebebef. Zeron Dark seeds (`builtins.rs:240-257`): background #060606, shell #0d0d0d, raised #343438, card #0e0e0e, text #e8e8ea, muted #a9a9ae, faint #85858a, accent #8b7cf6, danger #f87171, warning #facc15, success #34d399.

Type (`crates/ui/src/typography.rs:24-128`, `theme.rs:1208`): Geist / Geist Mono, UI sizes 12..18 selectable (`UiFontSize::ALL`), rems from 16 px base, code 12.5, terminal 13.

Motion (`crates/ui/src/motion.rs:353-480`): FADE_QUICK 150 ms ease, MENU_IN 140, MENU_OUT 100, DIALOG_IN 180, HOVER_FADE 150 `cubic-bezier(0.4,0,0.2,1)`; `CubicBezier` evaluator (`:267-281`); reduced motion via `App::reduce_motion` snaps all `with_animation` (`:17-23,1133-1141`), Linux reads portal `reduced-motion` (`:1329-1346`).

### 2.3 OpenVector token set (adaptation)
Constraints override Zeron where they conflict: true black, white text, no cards, no pulse/spinner/blur.

| Role | Value | Note |
|---|---|---|
| background, shell | #000000 | replaces #060606/#0d0d0d |
| raised (inputs, menus, dialogs) | #0e0e0e | Zeron card value |
| text | #ffffff | replaces #e8e8ea |
| text_muted | #a9a9ae | labels only, never subtitle lines |
| border / border_strong | white 0.10 / 0.18 | hairlines are the only chrome |
| hover / active | white 0.11 / accent 0.18 | |
| accent | #8b7cf6 | focus ring, selected tool, slider fill |
| danger / success / warning | #f87171 / #34d399 / #facc15 | removed-region outline uses danger |
| radius | control 6, dialog 10 | no pill radii |
| spacing | 4 / 8 / 12 / 16 | dense: rows 28, buttons 28, icon buttons 28x28 |
| header / footer | 38 / 24 | titlebar height and status strip |
| type | Geist 13 UI, Geist Mono 12 numerics | tabular readouts |
| motion | 0 ms default; menus/dialogs 140/100/180 ms with `cubic-bezier(0.175, 0.885, 0.32, 1.1)`; none when reduced | no loops; progress is a static bar |

No frost/BackdropBlur (Linux opaque anyway, `docs/theme-system.md`), no glass, no badges/pills.

### 2.4 Components and layout patterns to mirror
- Shell: titlebar row, content, 24 px status strip (`theme.rs:819-825`) maps to header/workspace/footer of `app.jsx:701-746`.
- Rail (`crates/ui/src/rail.rs`) for the viewer tool column; popover/picker (`popover.rs`, `pickers.rs`) for MenuSelect; settings rows (`settings.rs`, theme doc "ordinary settings rows", right-aligned compact controls) for wizard fields; toast/notice (`toast.rs`, `notice.rs`) for status; `image_viewer.rs` for pan/zoom surface.
- Icons: Zeron embeds its own SVG set (`crates/ui/src/icons.rs`, "Zeron Icons (icons.zeron.sh)" `motion.rs:470`). OpenVector keeps Lucide (current preview) as embedded SVG assets rendered with `gpui::svg()`, 14-16 px, stroke 1.5, text color.

## 3. GPUI dependency, build, tests

### 3.1 Options (sources)
| Option | Version | License | Source |
|---|---|---|---|
| `gpui` on crates.io (official) | 0.2.2, pre-1.0, frequent breaks | Apache-2.0 | https://crates.io/crates/gpui, https://docs.rs/crate/gpui/latest |
| Zed git | `zed-industries/zed` `crates/gpui`, pin a `rev` | Apache-2.0 | https://github.com/zed-industries/zed/tree/main/crates/gpui |
| `gpui-pre` snapshots | `=0.3.9` (+ `gpui-pre-platform`) | Apache-2.0 | https://github.com/longbridge/gpui-component/blob/main/Cargo.toml (lines 62-78) |
| Zeron fork `zeronsh/zui` | rev 361ff20 | follows Zed | https://github.com/zeronsh/zeron/blob/main/Cargo.toml |
| `gpui-component` (Longbridge) | crates.io 0.7.3; repo main 0.8.0 | Apache-2.0 | https://crates.io/crates/gpui-component, https://lib.rs/crates/gpui-component |

Decision: depend on Zed GPUI by exact git rev, because crates.io 0.2.2 lags Zed main and `TestAppContext`/`VisualTestContext` live under the `test-support` feature on main (`crates/gpui/Cargo.toml:21`). Use `gpui-component` only if its pinned GPUI matches; since repo main pins `gpui-pre =0.3.9` (a different package name), mixing it with a Zed git rev duplicates GPUI. Therefore pick one of: (a) Zed git rev, own components (recommended; the UI is small, dense and custom-styled; shadcn look is reproduced in ~12 components); or (b) `gpui-component` 0.7.3 plus its exact `gpui-pre` pin, adopting its Input, Slider, Checkbox, Select, Dialog, Scrollbar. Option (a) also keeps the dependency audit surface smaller for SOC 2 / ISO 27001 change management: exact `rev` pins, `Cargo.lock` committed, `cargo deny` for licenses (Apache-2.0/MIT only) and advisories.

```toml
# rust/Cargo.toml [workspace.dependencies]
gpui = { git = "https://github.com/zed-industries/zed", rev = "<verified sha>" }
# dev-dependencies of openvector-gui
gpui = { git = "https://github.com/zed-industries/zed", rev = "<same sha>", features = ["test-support"] }
```
Record the verified SHA and date in `docs/rust-port-contract.json` `upstream.versionDecision`.

### 3.2 Ubuntu build deps
From Zeron CI (`.github/workflows/ui-tests.yml:66-70,85-90`):
```
sudo apt-get install -y libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
  libx11-dev libxcb1-dev libx11-xcb-dev libfontconfig1-dev libfreetype-dev \
  libvulkan-dev pkg-config cmake
# headed smoke runs:
sudo apt-get install -y xvfb xauth xdotool mesa-vulkan-drivers
```
(`libasound2-dev`, `libwebkit2gtk-4.1-dev` are Zeron-specific, not needed.) Host is arm64 Linux; GPUI Linux renderer uses Vulkan (blade/wgpu), so headed runs need a Vulkan ICD (lavapipe via `mesa-vulkan-drivers`) under `xvfb-run`.

### 3.3 Headless tests
`crates/gpui/src/app/test_context.rs` (Zed main): `TestAppContext` (`:21`), `add_window` (`:279`), `add_empty_window` (`:326`), `add_window_view -> (Entity<V>, &mut VisualTestContext)` (`:347`), `run_until_parked` (`:595,884`), `simulate_keystrokes` (`:617,913`), `simulate_input` (`:633,919`), `VisualTestContext` (`:857`), `simulate_mouse_down` (`:938`). Tests use `#[gpui::test] fn t(cx: &mut TestAppContext)` (as in Zeron `motion.rs:1448-1538`). These run on the test platform with no GPU or display, suitable for CI. Reduced motion is settable via `cx.set_reduce_motion`.

## 4. Crate layout, module map, checks

### 4.1 Layout
```
rust/crates/openvector-gui/
  Cargo.toml            # bin "openvector", deps: openvector (engine), gpui, serde, rfd (native dialogs)
  assets/icons/*.svg    # Lucide subset, assets/fonts/Geist*.ttf (OFL)
  src/main.rs           # App::new, register fonts/assets, open window, keymap
  src/theme.rs          # tokens of 2.3, motion spec, reduced-motion global
  src/actions.rs        # gpui actions + keymap (1.2, viewer B/V/Shift)
  src/app_state.rs      # workspace mode, document, settings owner, preferences (one ImageDocument/CoreSession)
  src/jobs.rs           # background executor jobs, progress, cancel flag
  src/components/{button,icon_button,select,slider,checkbox,number_field,choice,dialog,disclosure,status_line,progress,scrollbar}.rs
  src/shell/{header,footer,drop_overlay}.rs
  src/viewer/{mod,geometry,zoom,input,layers,paint}.rs
  src/wizard/{mod,nav,pages/*.rs}.rs
  src/batch/{mod,files,settings,progress,summary}.rs
  src/profiles.rs  src/quick_save.rs  src/bitmap_export.rs  src/sources.rs  src/dialogs.rs
  tests/workflows.rs    # TestAppContext workflows below
```
Engine logic stays in `crates/openvector`; GUI holds view state only. Ports of `gui/*.mjs` controllers become plain Rust modules tested without GPUI.

### 4.2 Screen to module map
| Vector Magic / preview element | Module | JS reference |
|---|---|---|
| App, workspace modes, startup load | `main.rs`, `app_state.rs` | `app.jsx:57-107`, `gui/application-controller.mjs` |
| Header, footer, drop | `shell/*` | `app.jsx:697-747` |
| Viewer rail, viewport, scrollbars | `viewer/mod.rs`, `components/scrollbar.rs` | `viewer.jsx:27-180` |
| Zoom slider/wheel/fit/100% | `viewer/zoom.rs`, `viewer/geometry.rs` | `gui/viewer-zoom*.mjs`, `gui/viewer-geometry.mjs` |
| B/V/Shift, pointer chords | `viewer/input.rs` | `gui/viewer-input.mjs` |
| Layers, paths, removed outlines | `viewer/layers.rs`, `viewer/paint.rs` | `viewer.jsx:144-174` |
| Wizard frame + nav graph | `wizard/mod.rs`, `wizard/nav.rs` | `wizard.jsx:362-380`, `app.jsx:483-511`, `gui/review-state.mjs` |
| Pages load..export | `wizard/pages/{load,mode,image_type,transparency,quality,colors,palette,segmentation,smoothing,editing,background,review,export}.rs` | `wizard.jsx:213-360` |
| Auto-Update | `wizard/mod.rs` | `gui/auto-update.mjs` |
| Saved settings | `profiles.rs` | `wizard.jsx:111-157`, `gui/profile-controller.mjs` |
| Quick Save | `quick_save.rs` | `gui/quick-save*.mjs` |
| Bitmap export | `bitmap_export.rs` | `wizard.jsx:61-99`, `gui/bitmap-size.mjs` |
| Recent directories | `sources.rs` | `gui/source-controller.mjs` |
| Batch pages | `batch/*` | `batch.jsx`, `gui/batch-controller.mjs` |
| Processing progress/cancel | `jobs.rs` | `gui/processing-job.mjs`, `app.jsx:157-166` |
| Confirmations, About | `dialogs.rs` | `app.jsx:25-55,711-715` |
| Numeric text fields | `components/number_field.rs` | `gui/numeric-text.mjs` |

### 4.3 Executable workflow checks (`cargo test -p openvector-gui`)
Each is a `#[gpui::test]` using `add_window_view` + `simulate_keystrokes`/mouse events and asserts state snapshots against existing fixtures under `test/fixtures/` where named.
1. Startup with empty preferences shows Load page, Export disabled, footer "Ready".
2. Open PNG fixture: header shows name and W x H; viewer fits (Fit formula, 17 px extent) and slider synced.
3. Fully automatic: processing completes, page Review (or Export with skipReview), vector layer enabled.
4. Basic path: mode, type, transparency only for alpha source, quality, colors, palette when fixed; Back reverses per `review-ui-contract.json:23`.
5. Advanced path: palette, segmentation, smoothing with Auto-Update toggles; requestUpdate when controls differ.
6. Zoom: all 301 slider scales equal `test/fixtures/viewer-zoom-reference.ndjson.gz`; wheel trunc(delta/12) with anchor; 100% exact 1.
7. Viewer key state: replay the six native sequences in `test/fixtures/viewer-input-reference.ndjson.gz` (99 actions).
8. Pointer chords: replay `test/fixtures/browser-pointer-chords.json`; Pan ends on any release.
9. Editing: Pencil, Fill, Zap, Pick color gestures; Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y; shortcuts ignored while a text field has focus.
10. Remove background: erase, undo, outlines drawn with danger color, export respects visibility.
11. Review controls change quality/colors and route per review contract; Skip persists across restart.
12. Export each format AI, DXF, EMF, EPS, PDF, SVG to a temp dir; bytes equal engine CLI output for the same settings.
13. Overwrite dialog: Cancel keeps file, Overwrite replaces; focus returns to opener; Escape equals Cancel.
14. Quick Save basic vs advanced choices independent and persisted; cancellation leaves no partial file.
15. Bitmap export: size field commit/validation, Use view scale, flatten acknowledgement.
16. Saved settings: save exact name, overwrite confirm, rename, delete, Play, Fast Forward.
17. Batch: add files (duplicates reported), settings ranges basic 2..12 vs advanced 2..24, start, overwrite decide, cancel at palette checkpoint, retry, summary and log.
18. Single-image commands rejected while batch job runs.
19. Processing Cancel from footer: status Cancelled, document retained.
20. Error path: corrupt file shows `role=alert` equivalent status text and keeps prior document.
21. Keyboard: Alt+R, PageUp/PageDown, Alt+B/N, Enter navigate; Tab order reaches every control; each control has an accessible label.
22. Theme: root background #000000, primary text #ffffff, no element uses radius above 10, no `with_animation` registered when reduce_motion is true, no repeating animation ever.
23. Headed smoke (CI job): `xvfb-run -a cargo run -p openvector-gui -- --smoke fixture.png` opens, processes, exports SVG, exits 0.

## Sources
- Repo files cited inline (`gui/src/*.jsx`, `gui/*.mjs`, `docs/*-contract.json`, `rust/Cargo.toml`).
- https://github.com/zeronsh/zeron : `Cargo.toml`, `docs/theme-system.md`, `crates/ui/src/theme.rs`, `crates/ui/src/typography.rs`, `crates/ui/src/motion.rs`, `crates/theme/src/builtins.rs`, `.github/workflows/ui-tests.yml`, `.github/actions/linux-ci/action.yml`.
- https://github.com/zed-industries/zed : `crates/gpui/Cargo.toml`, `crates/gpui/src/app/test_context.rs`.
- https://github.com/longbridge/gpui-component/blob/main/Cargo.toml
- https://crates.io/crates/gpui (0.2.2), https://docs.rs/crate/gpui/latest, https://crates.io/crates/gpui-component (0.7.3, Apache-2.0), https://lib.rs/crates/gpui-component
