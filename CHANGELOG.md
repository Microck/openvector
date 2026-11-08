# Changelog

Notable changes to OpenVector follow [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions use [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Dates are the GitHub release publication dates in UTC.

## [Unreleased]

## [0.1.3] - 2026-11-08

### Changed

- Reduce repeated pixel work during image-based smoothing while preserving evaluation order
- Accelerate exact 5×5 and 7×7 noise reduction, contour fitting, repeated-color palette assignment, and vector-export decimal formatting

### Fixed

- Correct corrupted colors when loading 16-bit interlaced RGB and RGBA PNG images
- Preserve painted colors when refitting segmentation edits on translucent images
- Keep short pencil strokes local to their sampled pixels

The smoothing cache adds eight bytes per image pixel. No combined-release speedup
is claimed. See the [release verification and coverage limits](https://github.com/Microck/openvector/releases/tag/v0.1.3).

## [0.1.2] - 2026-11-07

### Added

- Publish the CLI and Node.js library on npm for Linux amd64/arm64 and Windows x64, requiring Node.js 22.19 or later
- Expose text-field content and selections to assistive technology, including Unicode selection positions within graphemes

Full screen-reader support and Wayland remain unverified. This version was
published with missing private captures, incomplete strict-suite validation, no
fresh Windows 10 check, and incomplete manual platform coverage. See the
[release verification details](https://github.com/Microck/openvector/releases/tag/v0.1.2).

## [0.1.1] - 2026-11-06

### Added

- Provide Developer ID-signed, notarized, and stapled macOS downloads for Apple Silicon and Intel

### Changed

- Show the OpenVector icon in Windows setup and let you choose the installation folder
- Remove the SVG dimensions note from export screens without changing exported dimensions

### Fixed

- Avoid flashing the preparation label when changing drag formats for fast exports

## [0.1.0] - 2026-11-06

### Added

- Provide Linux amd64 archives and a Windows x64 installer alongside the portable downloads

### Fixed

- Retry busy files during Windows uninstall and preserve the installation and its registration when deletion remains blocked
- Preserve unrelated files during Windows uninstall
- Identify OpenVector in EMF exports without captured application metadata
- Use the OpenVector producer field in JavaScript PDF exports, matching the native engine

macOS downloads were withheld for this version. See the
[release coverage limits](https://github.com/Microck/openvector/releases/tag/v0.1.0).

## [0.0.0] - 2026-10-29

### Added

- Trace raster images, edit palettes and segmentation, and remove backgrounds
- Export SVG, PDF, EPS, AI, DXF, and EMF, or save raster output
- Convert batches and retain advanced profiles and recent basic settings
- Provide Windows x64 and Linux ARM64 desktop downloads with matching source, license notices, checksums, and build provenance

### Fixed

- Continue Windows scrollbar paging through held-button capture transfer and stop at the pointer
- Preserve the previous image and its export state when opening another image fails

This initial desktop release did not include macOS downloads or an npm package.
Its checks did not establish full reference parity or an entirely green strict
native suite. See the [historical verification report](https://github.com/Microck/openvector/blob/v0.0.0/docs/verification.md).

[Unreleased]: https://github.com/Microck/openvector/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/Microck/openvector/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/Microck/openvector/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/Microck/openvector/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Microck/openvector/compare/v0.0.0...v0.1.0
[0.0.0]: https://github.com/Microck/openvector/releases/tag/v0.0.0
