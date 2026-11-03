# Verification pass

Use the exact baseline or candidate binary and record its SHA-256 and source commit.
Launch a separate instance with OPENVECTOR_PREFERENCES pointing to task-owned state.
Use disposable source copies and output files. Preserve the user's existing app.
Run P1 first, then P2, then P3. Record pass, fail or blocked with evidence; every
failure belongs in ../bug-triage.md, including documentation errors.

Each checklist uses ID | P | Needs | Claim | Setup | Steps | Expected | Result.
P1 covers established facts and suspected bugs; P2 ordinary behavior; P3 exact
numbers, color and timing. No document is verified solely by a unit test or static
reading. Keep unfinished and unsupported combinations explicit.
