# 0.1.2

- text fields expose content and selections to assistive technology, including unicode selection positions inside graphemes.
- linux at-spi diagnostics check named controls, dialogs, focus and retained numeric edits in isolated sessions.
- full screen-reader support, wayland and cross-platform screen readers remain unverified.
- released with documented coverage limits: two missing private captures, incomplete strict-suite validation, no fresh windows 10 check, and an incomplete manual native workflow matrix. see the [release verification details](https://github.com/Microck/openvector/releases/tag/v0.1.2).

# 0.1.1

- windows setup uses the openvector icon and lets you choose the installation folder.
- export screens no longer show the svg dimensions note. exported dimensions are unchanged.
- changing drag formats no longer flashes the preparation label for fast exports.

# 0.1.0

- emf exports identify openvector and contain no captured application metadata.
- the javascript pdf writer now uses the same openvector producer field as the native engine.
- development rebuilds verify bundled codec sources against retained upstream identity manifests.

# 0.0.0

first private desktop release of openvector.

- trace raster images, edit palettes and segmentation, and remove backgrounds.
- export svg, pdf, eps, ai, dxf and emf, or save raster output.
- convert batches and keep advanced profiles and recent basic settings.
- windows scrollbar paging continues through held-button capture transfer and stops at the pointer.
- a failed open keeps the previous image and its export state.

## downloads

- **windows x64:** extract the whole zip and open `openvector-gui.exe`. tested on windows 10; the executable is unsigned.
- **linux arm64:** extract the tarball and run `openvector-gui-linux/openvector-gui`. requires glibc 2.39 or later and the libraries in [the build guide](https://github.com/Microck/openvector/blob/v0.0.0/docs/building.md).
- checksums and build provenance accompany the archives. keep the included license notices with the app.

macos and other architectures require a source build. no npm package is published.
the source archive includes the engine, cli, node binding and editable third-party sources.

## tested scope

these are retained, verified 0.0.0 binaries. windows conversion and viewport workflows
and linux packaged gui conversion passed within the documented scope. this release
does not claim full vector magic parity or an entirely green strict native suite.
see [verification and remaining limits](https://github.com/Microck/openvector/blob/v0.0.0/docs/verification.md).
