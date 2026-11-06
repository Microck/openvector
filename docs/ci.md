# GitHub Actions source CI

[The source workflow](../.github/workflows/source.yml) runs on GitHub-hosted
`ubuntu-24.04` runners for pull requests and pushes to `main`. It uses Node.js 24
and runs the same command available locally:

```sh
bash tools/check-source.sh
```

This installs locked npm dependencies without lifecycle scripts, runs the
small-reference and independently authored smoke checks, checks JavaScript syntax,
and inspects the npm source package inventory without building native binaries.
The job has a ten-minute timeout. New commits cancel older runs for the same
pull request or branch.

The workflow has read-only repository permissions, does not persist checkout
credentials, and needs no repository secrets or self-hosted runners. Actions are
pinned to commit hashes. Fork pull requests run through `pull_request`, not
`pull_request_target`.

A green `source` check covers these checks only. Native, full differential and
graphical checks remain separate [release requirements](release-runbook.md).
The larger fixture corpus is not downloaded by CI. See
[test fixtures](test-fixtures.md) for its requirements.
