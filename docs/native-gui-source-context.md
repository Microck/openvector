# Native GUI source-context investigation

The 128x128 own anchored texture initially failed the private automatic
comparison: final random state 7881 versus native GUI 311. Classification and
selected controls agreed. The original failure remains in the evidence.

Operation 110 records actual completed workers after joining QThread and before
its original queued completion runs. The trace uses static storage and appends
after the full response. Two fresh traces repeat exactly, and every byte of the
prior complete GUI response remains unchanged. No native callback, instruction
or license check is replaced. Changed event pacing does not establish in-flight
scheduler behavior.

The raw source-context word is 2 before Auto. It remains 2 after each worker.
Classification ends at random state 9142. Segmentation creates 1530 regions;
smoothing and fitting end at 311. The separate native controller and independent
pipeline with explicit context 2 match all final fields, all 100 parameters,
source/working pixels, segmentation bits, contours, smoothing metadata, fitted
geometry and output chains.

With explicit context 0, both the native controller and independent pipeline
instead create 1529 regions and finish at 7881. This isolates the mismatch to a
source-context input. No numerical-kernel patch is justified by this case.

The rejected edit-stage hypothesis is also retained. Native 0x46a440 produces
an empty key for an empty edit list. Since the document key is also empty,
0x46b780 skips the bitmap rebuild. It does not explain this mismatch.

The encoded-file producer is now recovered. LoadImageThread 0x436ff0 calls the
document reader 0x46a9a0, which commits the source name through 0x46e890 and Shared
setter 0x484b00. The setter parses the filename, applies extension aliases and
initializes the context to -1 before its four-entry lookup. The authoritative
rule is in image-state-contract.json under sourceContextProducerRecovery.

Operation 111 verifies fifteen actual MainWindow loads with identical own PNG
bytes under different names. It covers all mapped context values, unknown
suffixes, case, aliases, dotfiles, a long suffix, a dotted parent and mixed
separators. Both complete fresh responses repeat. Decoded pixels match the
independent decoder; the original installation and two saved numeric profiles
remain unchanged. These are filename tests, not additional decoder coverage.

Canonical resolveImageFilename now supplies both extension and shadowControl to
file consumers. The library exposes it with decodeImage, and GuiEngine.open uses
it. The full larger-texture comparison passes through this filename path without
an override. Existing raw decoded-source APIs still require explicit context;
their controlled zero-input corpus remains valid. Historical zero-context file
comparison failures remain preserved, rather than being reclassified as passes.

The complete file corpus now matches 36 Auto actions and 3600 typed parameter
records. The fresh installed package independently repeats 35 actions/3500
records with original/workspace engine reads and child creation denied. The
large source run and byte-identical package verification are recorded separately.
Observers receive owned copies and cannot mutate processing. Product Auto is
still disabled pending one shared public workflow operation. Nothing has been
published to GitHub or npm.

Evidence: ../evidence/source-context-verification-checkpoint.json and
../evidence/native-gui-large-texture-first-divergence.json.
