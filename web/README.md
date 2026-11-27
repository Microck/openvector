# OpenVector in the browser

The website runs the desktop Rust application with GPUI's browser renderer. The wizard, toolbars, editor, and export screens come from `openvector-gui`. Images and settings live in a private, in-memory filesystem for the lifetime of the tab. The browser handles file selection and downloads.

## Build and run

On Linux, install Python 3, Node 22 or later with npm, WASI SDK 25, Rust nightly dated 2026-11-26, and `wasm-bindgen-cli` 0.2.129. From this directory, run:

```sh
rustup toolchain install nightly-2026-11-26 --profile minimal --target wasm32-wasip1-threads --component rust-docs
npm ci
WASI_SDK_PATH=/path/to/wasi-sdk npm run build
python3 serve.py --port 4174
```

Serve the contents of `dist/` over HTTPS or localhost. Send `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` on `/editor/`, and `application/wasm` for the Wasm file. The development server supplies these headers. The editor requires WebGL2 for rendering and shared WebAssembly memory for tracing.

The editor and its document worker share the Rust command and progress channels. A trace runs on the worker while GPUI paints and handles input on the browser thread. Both use the same in-memory filesystem. The build enables JavaScript bindings in three pinned crates for this WASI target and keeps the native workspace lockfile intact.

To check cancellation, UI responsiveness, and native SVG parity through the running editor, run `python3 check-worker.py http://localhost:4174/editor/`. The check uses Python Playwright, Chromium, and Pillow. Set `CHROMIUM` to use an installed Chromium executable. Pass `--browser firefox` or `--browser webkit` to run the same check on those engines. Use `--headed` when the engine needs a display for WebGL2.

### Export parity

Run `python3 check-exports.py http://localhost:4174/editor/` for the six browser
downloads and the JPEG session regression in Chromium. It uses the same
Playwright, Pillow, and optional `CHROMIUM` settings as the worker check.
A fresh editor and a fresh native CLI process start with the same smoothing
state. When comparing a sequence of imports, replay earlier traces in one
native session as well. The browser uses a 96 CSS-pixel inch for the EMF
reference device (960 pixels per 254 mm on both axes). Native EMF comparisons
must use equivalent display metrics. EPS includes the local creation time.

## Deploy

The static landing page in `site/` is served at `/`, the editor at `/editor/`,
and the notices at `/licenses/`. On Cloudflare Pages, `site/_redirects` sends the launch-era `/app/` paths to `/editor/`. If the domain changes, update the canonical
URL, social tags and structured data in the landing page along with
`llms.txt`, `robots.txt` and `sitemap.xml`.

For Cloudflare Pages, use Direct Upload with the completed release directory as
the upload root. Build on the configured Linux build host first. The bundle
includes a Pages `_headers` file for the editor and its workers. Each uploaded
asset must be at most 25 MiB. Check the complete release, including the source
archive, before uploading. For an existing Pages project, deploy with
`wrangler pages deploy /path/to/new-release --project-name YOUR_PROJECT`.
Keep the deployment static on Pages Free. Browser workers run on the user's
device. No Pages Functions, database, or paid storage are required. The build
gives the Wasm engine a content-based URL under `/editor/engine/` so browsers can
cache it for a year; the launcher revalidates and points at the current engine.
On a preview deployment verify `window.crossOriginIsolated` and run the
worker check above before promoting it.

Serve a single completed `dist/` directory as the site root. For Caddy, set
`OPENVECTOR_SITE` to the public hostname and `OPENVECTOR_DIST` to the absolute
path to that directory, then load the supplied Caddyfile. It enables HTTPS,
the isolation headers, cache revalidation, and compressed delivery. Switch
releases together so the worker, JavaScript bindings, and Wasm match.

The build includes the MIT license, third-party notices, and a source archive
at `/licenses/`. The source includes the Qt-derived stroke code and the build
scripts needed to change it and rebuild the editor. Cargo and npm download the
locked dependencies during a rebuild. Keep these files with every deployment.

Use `npm run build -- --out /path/to/new-release` to stage a separate release.
The default build replaces the generated `dist/` directory. A custom output path must be new.

## Opening-screen sponsorship

An unbooked opening screen invites sponsors to email sponsor@openvector.app,
which Cloudflare Email Routing forwards to openvector@micr.dev. To book the
slot, replace the contents of `#opening-sponsor` in `index.html` with the
labelled sponsor link from the comment beside it. Use plain text and a direct
HTTPS destination URL. Restore the invitation when the booking ends. The slot
appears on roomy opening screens and disappears on the first editor
interaction.

## Network ads

Network ads belong on the landing page, which is not cross-origin isolated.
Only `/editor/` sends the isolation headers, so ad scripts and frames load on `/`.
Keep provider JavaScript out of the editor.
The landing page loads Google AdSense (`ca-pub-1182866151080746`), and `site/ads.txt` authorizes it.
Its one display unit (`landing-display`, slot `1491555265`) sits between the CLI and Questions sections and hides itself when unfilled.
Google's consent message (AdSense Privacy & messaging) covers visitors in the EEA, the UK and Switzerland.
