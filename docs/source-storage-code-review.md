# Source storage and cached thumbnails

The source host owns real disk paths, one recent-file metadata owner, cached
thumbnail bytes and one JSON storage file. Startup restores cached entries without
reading their image files. Opens use the existing engine decode and source-load
path. Byte-only uploads never create fictitious disk-history entries.

## Thumbnail failure investigation

| Check | Finding |
| --- | --- |
| Expected behavior | Reopening a changed image refreshes its cached label and thumbnail. |
| Invariant | A solid-color image keeps its color and alpha when scaled. |
| What definitely did not happen | Startup did not reopen or rescan the missing or changed files. |
| Bug class | Incorrect thumbnail arithmetic. |
| Causal chain | Explicit reopen, engine decode, floating-point thumbnail sampling, incorrect cached pixels. |
| First unintended side effect | Weighted sums rounded below an integer before truncation. |
| Canonical source of truth | The original application's PNG thumbnails retained in its private settings namespace. |
| Competing sources | No second history owner; the former floating-point sampler supplied incorrect derived pixels. |
| Symptom | Some solid-color pixels and alpha values changed. |
| Trigger | A 17x11 source enlarged to 32x20. |
| Root cause | Continuous floating-point weights did not reproduce the original fixed-point sampling and truncation. |
| Correct layer | File-thumbnail generation during the existing decode. |
| Minimal fix | Integer sampling with native interpolation, area reduction and alpha-conversion truncation. |
| Architectural follow-up | Verify wider opaque and alpha cases before connecting recent-file widgets. |
| Applied patch | Fixed-point axis weights, intermediate row truncation and thumbnail-specific alpha conversion. |

The installed original reports QtGui 4.4.3.0. The public
[Qt 4.8 scaling source](https://github.com/qt/qt/blob/4.8/src/gui/painting/qimagescale.cpp)
was an investigation aid. It is a different version; native cached pixels, not
that source version, establish this checkpoint's behavior. The new fixture keeps
12 patterned images across enlargement, reduction and unchanged size. Every
cached RGBA byte matches before and after restart. Native recordings also retain
the original startup controls rather than replacing a variable prefix with a
cold-start prefix.

## Simplification review

The ce-simplify-code reuse, quality and efficiency rubrics ran inline in one
context. Independent review contexts: zero. This loses independent review
coverage; the three lenses are not independent corroboration.

- Reuse: zero changes. Existing `displayPixels` rounds document alpha conversion;
  reusing it for the file thumbnail changes cached pixels. Batch previews have a
  different size and sampling contract. Both proposed consolidations were skipped.
- Quality: one change. Removed a redundant empty map allocation that the source
  controller constructor immediately replaced with its restored icon map.
- Efficiency: one change. Hoisted the invariant row divisor out of the pixel loop.
- No newly unused production functions, imports or exports were found. The host's
  single-image picker is a supported API command awaiting its GUI controls.

The settled code passes six local metadata-owner tests, the same six tests from
an offline-installed package, and all 321 GUI tests against a fresh isolated
build. Fifteen GUI tests cover source storage and its real worker/HTTP boundary.
The Windows picker C# compiles, but no native picker interaction is claimed.
No lint or typecheck script exists in either package. The workspace's served
build was left intact; its stale Ambivalence HTML explains the one failure in
the initial workspace GUI run. The fresh isolated build passes that same test.

Full parity remains unproven. Recent-file widgets, clipboard acquisition, native
file-error dialog outcomes, complete selected-settings persistence and other
application gaps remain. No GitHub or npm publication occurred.
